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
use std::comm::{Data, Empty, Disconnected};

use std::cast::transmute_mut;

use objectbroker;
use classloader::{ClassLoader, AbstractClassLoader};
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
	// Note: failure to locate the given class, method or object does 
	// not result in a failure to run the thread, but rather throws a
	// fatal exception in that thread, causing it to fail before it runs
	// any user code.
	//
	pub fn run_thread(&mut self, class : &str, method : &str, obj : Option<JavaObjectId>) -> Option<uint> {
		// Problem: if broker is already exited, broker_chan is hung up and
		// causes propagating failure as soon as ThreadContext registers.
		//
		// Solution: the broker cannot abandon the the broker_chan until we
		// acknowledge shutdown, which is strictly after the exit code is
		// capture in self.exit_code. As race conditions on self are impossible, 
		// a single check on is_exited() is sufficient.
		if self.is_exited() {
			return None;
		}

		// note: the ThreadContext immediately registers itself with the broker.
		// this prevents the VM from shutting down as the thread is non-daemon
		// by default.
		let ld = ~self.classloader.clone() as ~AbstractClassLoader;
		let mut t = ThreadContext::new(ld, self.broker_chan.clone());

		let tid = t.get_tid();
		t.set_context(class, method, obj);

		// this transfers ownership into a new task, which interprets the thread code
		t.execute();

		return Some(tid);
	}


	// ----------------------------------------------
	// Exit the VM if it is not EXITED. This interrupts all threads and
	// therefore forces them to terminate. This method inherently races with
	// running Java threads, which might as well terminate on their own.
	//
	// This is a blocking API. It returns the exit code of the Java program,
	// i.e. the value given to System.exit(), a 0 if all threads exited normally,
	// an implementation-defined negative value if a terminal exception caused the 
	// exit, and, if at the time exit() performs its duties the VM is not EXITED 
	// yet, another implementation-defined negative value.
	//
	// exit() is idempotent.
	pub fn exit(mut self) -> int {
		self.intern_await_exit();
		self.exit_code.unwrap()
		// because we own `self`, the destructor drop() gets called
	}


	// ----------------------------------------------
	// Convenience method for checking whether the VM is EXITED or not.
	// This is equivalent to checking whether get_exit_code() is Some()
	pub fn is_exited(&self) -> bool {
		self.get_exit_code().is_some()
	}



	// ----------------------------------------------
	// Check if the VM is in the EXITED lifetime state (see class docs for 
	// a more detailed explanation of possible lifetime states). This method 
	// is an inherent race condition. The return value is None if the VM is not 
	// exited yet and otherwise Some() of the exit code. See exit() for a 
	// description of exit codes.
	pub fn get_exit_code(&self) -> Option<int> {
		if self.exit_code.is_some() {
			return self.exit_code;
		} 

		// from outside view, is_exited() is a simple getter that does
		// not affect the visible behaviour of the VM. Internally,
		// is_exited() polls the broker's exit status and acknowledges
		// reception, allowing the broker to destruct itself. 
		let this = unsafe { transmute_mut(self) };
		
		match this.broker_port.try_recv() {
			Data(BROKER_TO_VM_DID_SHUTDOWN(code)) => {
				this.exit_code = Some(code);

				// acknowledge - this renders our broker chan and port hung up
				// but because exit_code is set we know not to use them.
				this.broker_chan.try_send(objectbroker::OB_VM_TO_BROKER(VM_TO_BROKER_ACK_SHUTDOWN));
				this.exit_code
			},

			Empty => None,
			Disconnected => fail!("logic error, broker cannot hang up unless we acked"),
		}
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
			while !self.is_exited() {}
		}
	}
} 


// proper cleanup once the VM goes out of scope
impl Drop for VM {
	fn drop(&mut self) {
		self.intern_await_exit();
	}
}


#[cfg(test)]
mod tests {
	use vm::*;
	use classloader::tests::*;

	#[test]
	fn test_vm_init_exit() {
		let mut v = VM::new(test_get_real_classloader());
		assert!(!v.is_exited());
		v.exit();
	}

	/* for now those do an endless loop

	#[test]
	fn test_vm_init_post_exit_access() {
		let mut v = VM::new(test_get_real_classloader());
		assert!(!v.is_exited());

		// CREATED -> RUNNING
		assert!(v.run_thread("","",None).is_some());

		// busy wait for EXITED, but do not kill the VM. This simulates what
		// happens if the VM is exited because of the Java program terminating.
		while !v.is_exited() {}

		assert!(v.is_exited());
		assert!(v.run_thread("","",None).is_none());
	}

	#[test]
	fn test_vm_init_threads_entrypoints_not_found() {
		// these threads are going to fail as the entry point cannot be found
		let mut v = VM::new(test_get_real_classloader());

		// CREATED -> RUNNING
		assert!(v.run_thread("","",None).is_some());
		assert!(v.run_thread("","",None).is_some());
		assert!(v.exit() < 0);
	}

	#[test]
	fn test_vm_init_threads_empty_main() {
		// these threads are going to succeed however - the corresponding program is empty
		let mut v = VM::new(test_get_real_classloader());
		assert!(v.run_thread("EmptyClassWithMain","main",None).is_some());
		assert!(v.run_thread("EmptyClassWithMain","main",None).is_some());
		assert_eq!(v.exit(), 0);
	} */
}
