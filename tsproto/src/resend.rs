use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};
use std::time::Instant;

use chrono::{Duration, Utc};
use futures::{self, Future, Sink};
use futures::task;
use tokio_core::reactor::Timeout;

use Error;
use handler_data::{Data, SendRecord};
use packets::*;

enum ResendState {
    /// Important for clients: The first packet is sent, but we got no response
    /// yet, so we don't know if the server exists.
    Connecting,
    /// Everything is clear, normal operation.
    Normal,
    /// No acks were received for a while, so only try to resend the next packet
    /// until the connection is stable again.
    ///
    /// No voice packets are sent in this mode.
    Stalling,
    /// Resending did not succeed for a longer time. Don't even try anymore.
    ///
    /// No voice packets are sent in this mode.
    Dead,
    /// Sent the packet to close the connection, but the acknowledgement was not
    /// yet received.
    Disconnecting,
}

/// Configure the length of timeouts.
pub struct TimeoutConfig {
    /// Interval to resend the first packet.
    pub connecting_interval: Duration,
    /// Timeout to give up sending the first packet and close the connection.
    pub connecting_timeout: Duration,
    /// Swith to [`Stalling`] when no awaited response was received after this
    /// duration (added to the current estimated response time).
    pub normal_timeout: Duration,
    /// Interval to resend the first packet in [`Stalling`] mode.
    pub stalling_interval: Duration,
    /// Switch to [`Dead`] when no awaited response was received after this
    /// duration.
    pub stalling_timeout: Duration,
    /// When in [`Dead`] state, close the connection after no packet is received
    /// for this duration.
    pub dead_timeout: Duration,
    /// When in [`Disconnecting`] state, close the connection after no packet is
    /// received for this duration.
    pub disconnect_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        TimeoutConfig {
            connecting_interval: Duration::seconds(1),
            connecting_timeout: Duration::seconds(5),
            normal_timeout: Duration::seconds(5),
            stalling_interval: Duration::seconds(5),
            stalling_timeout: Duration::seconds(30),
            dead_timeout: Duration::seconds(0),
            disconnect_timeout: Duration::seconds(5),
        }
    }
}

// TODO Implement resending as sink layer.
pub struct ResendFuture<CS> {
    data: Weak<RefCell<Data<CS>>>,
    sink: Box<Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>>,
    timeout_config: TimeoutConfig,
    /// The future to wake us up after a certain time.
    timeout: Timeout,
    /// If we are sending and should poll the sink.
    is_sending: bool,
    state: ResendState,
}

impl<CS> ResendFuture<CS> {
    pub fn new(
        data: Rc<RefCell<Data<CS>>>,
        sink: Box<Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>>,
    ) -> Self {
        Self::with_timeout_config(data, sink, TimeoutConfig::default())
    }

    pub fn with_timeout_config(
        data: Rc<RefCell<Data<CS>>>,
        sink: Box<Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>>,
        timeout_config: TimeoutConfig,
    ) -> Self {
        let handle = data.borrow().handle.clone();
        Self {
            data: Rc::downgrade(&data),
            sink,
            timeout_config,
            timeout: Timeout::new(
                Duration::seconds(1).to_std().unwrap(),
                &handle,
            ).unwrap(),
            is_sending: false,
            state: ResendState::Normal,
        }
    }
}

impl<CS> Future for ResendFuture<CS> {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        // Set task
        let data = if let Some(data) = self.data.upgrade() {
            data
        } else {
            return Ok(futures::Async::NotReady);
        };
        data.borrow_mut().resend_task = Some(task::current());
        if self.is_sending {
            if let futures::Async::Ready(()) = self.sink.poll_complete()? {
                self.is_sending = false;
            }
        }

        let logger = data.borrow().logger.clone();
        let now = Utc::now();
        while let Some(rec) = {
            let mut data = data.borrow_mut();
            // Print packet for debugging
            /*let mut q = ::std::collections::BinaryHeap::new();
            mem::swap(&mut q, &mut data.send_queue);
            let v = q.into_sorted_vec();
            info!(data.logger, "Send queue: {:?}", v.iter().map(|rec| {
                (rec.p_id, rec.next)
            }).collect::<Vec<_>>());
            data.send_queue.extend(v.into_iter());*/
            if let Some(rec) = data.send_queue.peek() {
                // Check if we should resend this packet
                if rec.next > now {
                    // Schedule next send
                    let dur = rec.next
                        .naive_utc()
                        .signed_duration_since(now.naive_utc());
                    let next = Instant::now() + dur.to_std().unwrap();
                    self.timeout.reset(next);
                    if let futures::Async::Ready(()) = self.timeout.poll()? {
                        task::current().notify();
                    }
                    return Ok(futures::Async::NotReady);
                }
            }
            data.send_queue.pop()
        } {
            // Try to resend this packet
            if let futures::AsyncSink::NotReady(_) =
                self.sink.start_send(rec.packet.clone())?
            {
                data.borrow_mut().send_queue.push(rec);
                break;
            } else {
                if let futures::Async::Ready(()) = self.sink.poll_complete()? {
                    self.is_sending = false;
                } else {
                    self.is_sending = true;
                }
                // Retransmission timeout
                let rto = {
                    // Get connection
                    let mut data = data.borrow_mut();
                    if let Some(con) = data.connections.get_mut(&rec.packet.0) {
                        // Double srtt on packet loss
                        if rec.tries != 0
                            && con.srtt < Duration::seconds(::TIMEOUT_SECONDS) {
                            con.srtt = con.srtt * 2;
                        }
                        con.srtt + con.srtt_dev * 4
                    } else {
                        // The connection is gone
                        continue;
                    }
                };
                let next = now + rto;
                let dur =
                    next.naive_utc().signed_duration_since(now.naive_utc());
                if dur > Duration::seconds(::TIMEOUT_SECONDS) {
                    warn!(logger, "Max resend timeout exceeded"; "p_id" => rec.p_id);
                    continue;
                }
                let rec = SendRecord {
                    next,
                    tries: rec.tries + 1,
                    ..rec
                };
                if rec.tries != 1 {
                    let to_s = if data.borrow().is_client { "S" } else { "C" };
                    warn!(logger, "Resend"; "p_id" => rec.p_id, "tries" => rec.tries, "next" => %rec.next, "to" => to_s);
                }
                data.borrow_mut().send_queue.push(rec);
            }
        }
        Ok(futures::Async::NotReady)
    }
}
