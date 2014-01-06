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

// Core VM API and thread management
// (but not actual bytecode interpretation - see thread.rs for this)

use std::hashmap::{HashMap};

use objectbroker::*;
use classloader::{ClassLoader};
use object::{JavaObjectId};
use thread::{ThreadContext};



// Primary Java Virtual Machine API
pub struct VM {
	priv obj_broker_chan : SharedChan<ObjectBrokerMessage>,
	priv classloader : ClassLoader,
}


impl VM {

	// ----------------------------------------------
	// 
	// 
	pub fn new(classloader : ClassLoader) -> VM {
		// construct an ObjectBroker. The broker, not the VM,
		// is the ultimate owner of all Java resources.
		let broker = ObjectBroker::new().launch();

		// register the VM with the object broker using the
		// "0" fake thread id.
		let (port, chan) = Chan::new();
		broker.send(OB_REGISTER(0, chan));

		VM {
			classloader : classloader,
			obj_broker_chan : broker
		}
	}


	// ----------------------------------------------
	// Spawn a new Java thread given a class, method and (optional but
	// required if the given method is an instance method) a Java object
	// to set as the *this* object for the method.
	// 
	// The thread is immediately able to run, but the exact time where
	// it starts is determined by the scheduler. run_thread() returns
	// immediately and does not wait for the thread to run.
	pub fn run_thread(&mut self, class : &str, method : &str, obj : Option<JavaObjectId>) {
		let t = ThreadContext::new(self.obj_broker_chan.clone());
		// TODO: setup method context etc
		t.execute();
	}


	// ----------------------------------------------
	// Exit the VM. This interrupts all threads and
	// therefore forces them to terminate.
	//
	// This is a synchronous API.
	pub fn exit(mut self) {
		// because we own `self`, the destructor drop() gets called
	}



	// IMPL

	fn intern_destroy(&mut self) {
		debug!("VM: exiting");
	}


//	pub fn get_exit_state() -> ExitState {
//
//	}
} 


// proper cleanup once the VM goes out of scope
impl Drop for VM {
	fn drop(&mut self) {
		self.intern_destroy();
	}
}














