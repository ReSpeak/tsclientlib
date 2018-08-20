#!/usr/bin/env bash
# Measure the latency of recording (a microphone) with the ts client, sending it
# to the server and to another client again and playing it.

if [[ -f ~/.TsLatencyPulseaudioLoop ]]; then
	echo Removing modules
	# Try to unload first
	while read -r line; do
		pactl unload-module $line
	done < ~/.TsLatencyPulseaudioLoop

	rm -f ~/.TsLatencyPulseaudioLoop
fi

set -e

if [[ -n $1 ]]; then
	exit
fi

# Either TeamSpeak3 or voice-client
from=voice-client
to=TeamSpeak3

# The sink for the music
pactl load-module module-null-sink sink_name=latency_nullsink sink_properties=device.description=LatencySink >> ~/.TsLatencyPulseaudioLoop
# The sink for TeamSpeak
pactl load-module module-null-sink sink_name=latency_tssink sink_properties=device.description=LatencyTsSink >> ~/.TsLatencyPulseaudioLoop

# Find source-output id of teamspeak
id=`pactl list source-outputs | grep "$from" -B 17 | grep "Source Output" | cut -d# -f2`
# Connect nullsink to teamspeak3
pactl move-source-output $id latency_nullsink.monitor

# Find source-output id of teamspeak
id=`pactl list sink-inputs | grep "$to" -B 17 | grep "Sink Input" | cut -d# -f2`
# Connect teamspeak3 to nullsink
pactl move-sink-input $id latency_tssink


# Now measure latency
gst-launch-1.0 -v pulsesrc device=latency_tssink.monitor ! audiolatency print-latency=true ! pulsesink device=latency_nullsink
