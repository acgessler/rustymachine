#!/bin/bash

# avoid obstructing output
export RUST_TEST_TASKS=1  
pushd bin
./rustyvm_test
popd