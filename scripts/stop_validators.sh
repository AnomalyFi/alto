#!/usr/bin/env bash

LOG_DIR=/tmp/alto-logs
PID_FILE=${LOG_DIR}/validators.pid
FLAG_FILE=${LOG_DIR}/network_running.flag

if [ ! -f ${PID_FILE} ]; then
    echo "No validators are running (PID file not found)."
    exit 1
fi

while read pid; do
    echo "Stopping validator process with PID ${pid}..."
    kill ${pid} 2>/dev/null
done < ${PID_FILE}

rm ${PID_FILE}
rm -f ${FLAG_FILE}
echo "All validators have been stopped and the network flag has been removed."
