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

use std::cast::transmute_mut;

use objectbroker;
use classloader::{ClassLoader};
use object::{JavaObjectId};
use thread::{ThreadContext};


// TODO: restructure so this becomes the main crate

pub enum VMToBrokerControlMessage {
	VM_TO_BROKER_DO_SHUTDOWN,
	VM_TO_BROKER_ACK_SHUTDOWN
}

pub enum BrokerToVMControlMessage {
	BROKER_TO_VM_DID_SHUTDOWN(int /* exit_code */ ),
}



// Primary Java Virtual Machine API

// The purpose of the VM is to provide a public API to initialize and
// administrate a virtual machine.
//
// A VM has a well-defined lifecycle:
//
// CREATED -> RUNNING -> EXITED
//
// Where
//
//   CREATED occurs after a VM has been created, but run_thread()
//   has not been called yet.
//
//   RUNNING occurs after run_thread() has been called at least
//   once and persists while at least one Java thread is running 
//   without the 'daemon' flag.
//
//   EXITED is the state in which the VM is placed if either
//     - the last Java thread without the 'daemon' flag terminates
//     - System.exit() is called
//     - exit() is invoked on the VM instance
//


pub struct VM {
	// Duplex connection to Broker task
	//
	// reason for this not being a extra::DuplexStream: this would
	// require broker to select() on multiple ports, which is
	//   a) not possible because Port<> is a type, not a trait and
	//      there is no generalized select() that would work on
	//		DuplexStream (could have our own DuplexStream though)
	//      or ports of different types at all.
	//   b) select() is slow and increases the broker task's overhead
	priv broker_chan : SharedChan<objectbroker::ObjectBrokerMessage>,
	priv broker_port : Port<BrokerToVMControlMessage>,

	priv classloader : ClassLoader,

	// If the VM is known to have exited, this is Some() of the exit
	// value. Otherwise, this is None. See exit()
	priv exit_code : Option<int>,
}


impl VM {

	// ----------------------------------------------
	// 
	// 
	pub fn new(classloader : ClassLoader) -> VM {
		// construct an ObjectBroker. The broker, not the VM,
		// is the ultimate owner of all Java resources.
		let (port, chan) = Chan::new();
		VM {
			classloader : classloader,
			broker_port : port,
			broker_chan : objectbroker::ObjectBroker::new(chan).launch(),
			exit_code   : None
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
	//
	// This method can be called multiple times on a VM, but not if the
	// VM is exited. As termination of a VM can be triggered by java code,
	// this is an inherent race condition which can be checked for using
	// is_exited() and through run_thread()'s return value.
	// 
	// None is returned if the VM was exited already, and Some(tid) otherwise
	// where tid is the unique identifier of the new thread. 
	//
	pub fn run_thread(&mut self, class : &str, method : &str, obj : Option<JavaObjectId>) -> Option<uint> {
		// Problem: if broker is already exited, broker_chan is hung up and
		// causes propagating failure as soon as ThreadContext registers.
		//
		// Solution: the broker cannot abandon the the broker_chan until we
		// acknowledge shutdown, which is strictly after the exit code is
		// capture in self.exit_code. As race conditions on self are impossible, 
		// a single check on is_exited() is sufficient.
		if self.is_exited().is_some() {
			return None;
		}

		// note: the ThreadContext immediately registers itself with the broker.
		// this prevents the VM from shutting down as the thread is non-daemon
		// by default.
		let t = ThreadContext::new(self.broker_chan.clone());
		let tid = t.get_tid();
		// TODO: setup method context etc

		// this transfers ownership into a new task, which interprets the thread code
		t.execute();

		return Some(tid);
	}


	// ----------------------------------------------
	// Exit the VM. This interrupts all threads and
	// therefore forces them to terminate.
	//
	// This is a blocking API. It returns the exit code of the Java program,
	// i.e. the value given to System.exit(), a 0 if all threads exited normally
	// and an undefined negative value if a terminal exception caused the exit.
	//
	// exit() is idempotent.
	pub fn exit(mut self) -> int {
		self.intern_await_exit();
		self.exit_code.unwrap()
		// because we own `self`, the destructor drop() gets called
	}



	// ----------------------------------------------
	// Check if the VM has exited already (see class docs for a more detailed
	// explanation of possible lifetime states). This method is an inherent
	// race condition. The return value is None if the VM is not exited yet
	// and otherwise Some() of the exit code. See exit() for a description
	// of exit codes.
	pub fn is_exited(&self) -> Option<int> {
		if self.exit_code.is_some() {
			return self.exit_code;
		} 

		// from outside view, is_exited() is a simple getter that does
		// not affect the visible behaviour of the VM. Internally,
		// is_exited() polls the broker's exit status and acknowledges
		// reception, allowing the broker to destruct itself. 
		let this = unsafe { transmute_mut(self) };

		loop {
			match this.broker_port.try_recv() {
				Some(BROKER_TO_VM_DID_SHUTDOWN(code)) => {
					this.exit_code = Some(code);

					// acknowledge - this renders our broker chan and port hung up
					// but because exit_code is set we know now to use them.
					this.broker_chan.try_send(objectbroker::OB_VM_TO_BROKER(VM_TO_BROKER_ACK_SHUTDOWN));
					return this.exit_code;
				},
				// since the broker cannot hang up before we acknowledge
				// (and control would not have reached here in this case)
				// this means there is no more message.
				None => (),
			}
		}
		return None;
	}


	// IMPL


	// ----------------------------------------------
	fn intern_await_exit(&mut self) {
		if self.exit_code.is_some() {
			return;
		} 

		debug!("VM: exiting");

		// send a SHUTDOWN message to the broker and wait for a SHUTDOWN
		// message as response, signalling that all threads have shut down.
		//
		// Ignore any failures happening on the way - we may be racing against
		// a Java thread calling System.exit().
		if self.broker_chan.try_send(objectbroker::OB_VM_TO_BROKER(VM_TO_BROKER_DO_SHUTDOWN)) {
			while self.is_exited().is_none() {}
		}
	}
} 


// proper cleanup once the VM goes out of scope
impl Drop for VM {
	fn drop(&mut self) {
		self.intern_await_exit();
	}
}














