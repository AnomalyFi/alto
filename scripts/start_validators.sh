#!/usr/bin/env bash

cd ./chain

NUM_VALIDATORS=${NUM_VALIDATORS:-5}
LOG_DIR=/tmp/alto-logs
PID_FILE=${LOG_DIR}/validators.pid
FLAG_FILE=${LOG_DIR}/network_running.flag

# Check if a network is already running
if [ -f ${FLAG_FILE} ]; then
    echo "Existing network detected (flag file ${FLAG_FILE} exists). Aborting launch."
    exit 1
fi

# Create log directory if it doesn't exist and clear any old PID file
mkdir -p ${LOG_DIR}
[ -f ${PID_FILE} ] && rm ${PID_FILE}

# Create the flag file to indicate that the network is running
touch ${FLAG_FILE}

for i in $(seq 0 $(($NUM_VALIDATORS - 1))); do
    echo "Launching validator $i..."
    cargo run --bin validator -- --peers assets/peers.yaml --config assets/validator${i}.yaml > ${LOG_DIR}/validator${i}.log 2>&1 &
    echo $! >> ${PID_FILE}
done

echo "Validators launched. PIDs stored in ${PID_FILE}."
