#!/usr/bin/env bash

CWD=$(pwd)

cd chain && cargo build --bin validator --target x86_64-unknown-linux-gnu --target-dir ../target 
cp ../target/x86_64-unknown-linux-gnu/debug/validator $CWD