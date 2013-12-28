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
    classld.add_from_classfile(*args.last());
}

