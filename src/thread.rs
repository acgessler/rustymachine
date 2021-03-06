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

// Implementation of the ThreadContext class, which interprets
// Java bytecode and thereby represents one java.lang.Thread


use std::sync::atomics::{atomic_add, AcqRel};

use std::hashmap::{HashMap};

use std::task::{task};

use objectbroker::*;

use localheap::{LocalHeap, JavaStrongObjectRef};

use classloader::{AbstractClassLoader};

use object::{JavaObjectId};



// A FrameInfo represents 
pub struct FrameInfo {
	// not necessaryily up-to-date for top of stack
	pc : uint,
	pc_opstack : uint,
	pc_locals : uint
}


// A context of execution in the VM, typically associated with, but
// not necessarily limited to, interpreting a java.lang.Thread
// instance.
pub struct ThreadContext {
	priv classloader : ~AbstractClassLoader,

	// id of this thread. Unique across all threads as it
	// is drawn from an atomic counter.
	priv tid : uint,

	priv heap : LocalHeap,

	// connection to object broker
	priv broker_port : Port<ObjectBrokerMessage>,
	priv broker_chan : SharedChan<ObjectBrokerMessage>,

	priv opstack : ~[u32],
	priv locals : ~[u32],

	priv frames : ~[FrameInfo],

	// marker variable to indicate that, during processing
	// of the current bytecode instruction, a message was
	// received that indicated that the VM is shutting
	// down.
	priv vm_was_shutdown : bool,

	// startup context for the thread. 
	priv startup_class : ~str,
	priv startup_method : ~str,
	priv startup_object : Option<JavaStrongObjectRef>
}

	// Thread ids start at 1 as 0 is reserved for the VM
static mut ThreadContextIdCounter : uint = 1;

impl ThreadContext {

	// ----------------------------------------------
	pub fn new(classloader : ~AbstractClassLoader, broker_chan : SharedChan<ObjectBrokerMessage>) -> ThreadContext 
	{
		// generate an unique thread id
		let id = unsafe {
			atomic_add(&mut ThreadContextIdCounter, 1, AcqRel)
		};

		// register this thread with the object broker
		let (port, chan) = Chan::new();
		broker_chan.send(OB_REGISTER(id, chan));

		let mut t = ThreadContext {
			classloader : classloader,
			tid : id,

			heap : LocalHeap::dummy(),
			broker_port : port,
			broker_chan : broker_chan,

			opstack : ~[],
			locals : ~[],
			frames : ~[],

			vm_was_shutdown : false,

			startup_class : ~"",
			startup_method : ~"",
			startup_object : None,
		};

		t.heap = unsafe { LocalHeap::new_with_owner(&mut t) };
		t
	}


	// ----------------------------------------------
	// Get the unique thread-id of the thread
	#[inline]
	pub fn get_tid(&self) -> uint {
		self.tid
	}


	// ----------------------------------------------
	// Set the context in which the java thread executes. This context
	// is not verified until the thread executes. If an object is
	// specified, it is required to exist though and its class type must
	// match the class specified.
	pub fn set_context(&mut self, class : &str, method : &str, obj : Option<JavaObjectId>) {
		self.startup_class = class.into_owned();
		self.startup_method = method.into_owned();

		self.startup_object = match obj {
			None => None,
			Some(jid) => Some(JavaStrongObjectRef::new(jid, &mut self.heap )),
		}
	}


	// ----------------------------------------------
	// Handle incoming messages from ObjectBroker until a message
	// satifies the given predicate. Messages are processed after
	// the predicate is consulted, but the message for which the
	// predicate returns true is still processed. 
	//
	// This method blocks until a message is received that satifies 
	// the predicate.
	//
	// The method returns false if, for some reason, the VM was 
	// terminated while processing messages. In such a case, 
	// the caller should fail silently and _not_ fail!() the task
	pub fn handle_messages_until(&mut self, pred : |o : &ObjectBrokerMessage| -> bool) -> bool {
		loop {
			let msg = self.broker_port.recv();
			let b = pred(&msg);

			self.handle_message(msg);
			if b || self.vm_was_shutdown {
				break;
			}
		}
		!self.vm_was_shutdown
	}


	// ----------------------------------------------
	// Sends a message to another thread via ObjectBroker, does 
	// not block.
	pub fn send_message(&self, msg : ObjectBrokerMessage) {
		self.broker_chan.send(msg);
	}


	// ----------------------------------------------
	pub fn die_exception(self, exception_type : &str, opt_message : Option<&str>)
	{
		// TODO
	}


	// ----------------------------------------------
	// Execute the context concurrently. This transfers ownership
	// of the context into a separate task and yields a communication
	// channel for other threads to interrupt.
	pub fn execute(mut self) {
		// important that task failure does not propagate
		let mut tt = task();
		tt.unwatched();

		do tt.spawn {
			let mut inner = self;

			// first: resolve the class
			let maybe_class = inner.classloader.load(inner.startup_class).await();
			match maybe_class {
				Err(msg) => {
					inner.die_exception("java.lang.ClassNotFoundException", None);
					return;
				},
				Ok(jclass) => {
					// resolve the method signature

				}
			}

			loop {
				inner.op();
				if inner.vm_was_shutdown {
					break;
				}
			}
			inner.die();
		}
	}


	// IMPL


	// ----------------------------------------------
	fn die(self) {
		// this thread dies and transfers all of its object to
		// the ownership of the broker. We do this _even_ in the
		// scenario that the whole VM is shutdown to enable this
		// process to be cancelled and operation to resume.
		let tid = self.tid;
		let chan = self.broker_chan.clone();
		let objects = self.heap.unwrap_objects();
		chan.send(OB_UNREGISTER(tid, objects));
	}


	// ----------------------------------------------
	#[inline]
	fn handle_message(&mut self, o : ObjectBrokerMessage) {
		match o {
			// those are not supported in this messaging direction
			// (i.e. they are only _sent_ to ObjectBroker)
			OB_REGISTER(a,b) => fail!("REGISTER message not expected here"),
			OB_UNREGISTER(a,b) => fail!("UNREGISTER message not expected here"),
			OB_VM_TO_BROKER(op) => fail!("OP_VM_TO_BROKER message not expected here"),

			OB_SHUTDOWN(a,b) => {
				// Since handle_message is called with a borrowed ref and
				// from a multitude of places, we keep the thread alive
				// until it touches execute() again, which then destroys it.
				self.vm_was_shutdown = true;
			},
			
			OB_REMOTE_OBJECT_OP(a,b,op) => 
				self.heap.handle_message(a,b,op),

			OB_THREAD_REMOTE_OP(a, b, remote_op) => {
				// TODO
			}
		}
	}


	// ----------------------------------------------
	#[inline]
	fn op(&mut self) {

	}
}

