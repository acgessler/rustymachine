#!/bin/sh
export RUST_TEST_TASKS=1 
export RUST_LOG=main=4 
pushd src
./main
popd