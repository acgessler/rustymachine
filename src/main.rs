// rustyVM - Java VM written in pure Rust
// Copyright (c) 2013 Alexander Gessler
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//

#[feature(globs)];

extern mod extra;
extern mod std;

use extra::getopts::{optopt, optflag, getopts, Opt};
use std::os;

use std::io::{println, File};

mod def;
mod util;
mod field;
mod method;
mod class;
mod classpath;
mod classloader;
mod code;
mod monitor;
mod object;
mod threadmanager;
mod objectbroker;
mod localheap;
mod thread;
mod vm;


fn print_usage(program: &str, _opts: &[Opt]) {
    println!("Usage: {} [options] main-class-name", program);
    println("-c\t\tExtra entries for CLASSPATH separated by ;");
    println("-h --help\tUsage");
}

fn main() {
	let args = os::args();
	let opts = ~[
        optopt("c"),
        optflag("h"),
        optflag("help")
    ];
    let matches = match getopts(args.tail(), opts) {
        Ok(m) => { m }
        Err(f) => { fail!(f.to_err_msg()) }
    };
    if matches.opt_present("h") || matches.opt_present("help") {
        print_usage(args[0], opts);
        return;
    }

    let classpath = match matches.opt_str("c") {
        Some(cpath) => cpath,
        None => ~""
    };
    
	let mut classld = classloader::ClassLoader::new_from_string(classpath);
    classld.add_from_classfile(*args.last().unwrap());
}

