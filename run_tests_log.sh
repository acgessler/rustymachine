#!/bin/sh

# avoid obstructing output
export RUST_TEST_TASKS=1  
export RUST_LOG=rustyvm_test=4 
pushd bin
./rustyvm_test
popd