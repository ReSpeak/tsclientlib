#!/usr/bin/env bash
set -e
# Measure the latency of recording (a microphone) with the ts client, sending it
# to the server and to another client again and playing it.

if [[ -f ~/.TsLatencyPulseaudioLoop ]]; then
	echo Removing modules
	# Try to unload first
	while read -r line; do
		pactl unload-module $line || true
	done < ~/.TsLatencyPulseaudioLoop

	rm -f ~/.TsLatencyPulseaudioLoop
fi

set -e

if [[ -n $1 ]]; then
	exit
fi

# Either TeamSpeak3 or voice-client
from='"audio-latency"'
to='"audio-latency"'

# The sink for the music
pactl load-module module-null-sink sink_name=latency_nullsink object.linger=1 media.class=Audio/Sink sink_properties=device.description=LatencySink >> ~/.TsLatencyPulseaudioLoop
# The sink for TeamSpeak
pactl load-module module-null-sink sink_name=latency_tssink object.linger=1 media.class=Audio/Sink sink_properties=device.description=LatencyTsSink >> ~/.TsLatencyPulseaudioLoop
echo Created modules

# Find source-output id of teamspeak
id=`pactl list source-outputs | grep "$from" -B20 | grep "Source Output" | cut -d# -f2`
echo Found output "$from": "$id"
# Connect nullsink to teamspeak3
pactl move-source-output $id latency_nullsink.monitor

# Find source-output id of teamspeak
id=`pactl list sink-inputs | grep "$to" -B20 | grep "Sink Input" | cut -d# -f2`
echo Found input "$to": "$id"
# Connect teamspeak3 to nullsink
pactl move-sink-input $id latency_tssink


# Now measure latency
echo Starting measurement
gst-launch-1.0 -v pulsesrc device=latency_tssink.monitor ! audiolatency print-latency=true ! pulsesink device=latency_nullsink
