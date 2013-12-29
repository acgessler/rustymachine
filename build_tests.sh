#!/bin/sh
clear
mkdir bin
rustc --test src/main.rs -o bin/rustyvm_test