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

use std::comm::{Chan, Port, SharedChan};
use std::hashmap::{HashMap};
use std::task::{task};

use extra::comm::{DuplexStream};



use object::{JavaObject, JavaObjectId};
use threadmanager::{ThreadManager, RemoteThreadOpMessage};
use threadmanager;
use vm;

// Enumerates all possible types of accessing objects.
// Threads wishing to acquire ownership of an object specify one of
// these access modi and are served accordingly.
#[deriving(Eq)]
pub enum RequestObjectAccessType {

	// Normal access - any access of an object's field requires
	// threads to own objects, which is made possible through
	// this access mode. Arbitrary modifications are possible,
	// but no guarantee is made that between two instructions on
	// one thread context not another thread kick in and obtains
	// access.
	OBJECT_ACCESS_Normal,

	// Request to also lock the object's monitor, thus enforcing
	// mutual exclusion with other threads who also go through
	// the monitor for accessing the object.
	//
	// Note that this does *not* actually lock() the monitor,
	// it only ensures that the object's monitor is not currently
	// lock by somebody else by the time the requesting thread
	// receives object ownership.
	OBJECT_ACCESS_Monitor,

	// Request to also lock the object's monitor, and to be given
	// preference over threads attempting to access with the
	// OBJECT_ACCESS_Monitor flag. This is used in response to
	// a wait() call on a monitor to make sure that such threads
	// are given preference over threads accessing a monitor from
	// outside.
	//
	// This also does *not* lock() the monitor.
	OBJECT_ACCESS_MonitorPriority,
}


// OB_REMOTE_OBJECT_OP() detail messages, enumerating all supported
// operations on remote (i.e. owned by other thread) objects.
pub enum RemoteObjectOpMessage {

	// request from thread a to addref object b. If the object 
	// is not known to the broker yet, this registers the object 
	// as being owned by thread a.
	REMOTE_ADD_REF,

	// request from thread a to release object b. If thread a 
	// owns this object, the message informs the broker that the
	// object has been deleted.
	REMOTE_RELEASE,

	// thread a asks which thread owns object b. Response is the 
	// same message with a (c, b) tuple, c is the id of the the 
	// owning thread.
	REMOTE_WHO_OWNS,

	// thread a asks to take over ownership of object b with the
	// given access type. When granted, a REMOTE_DISOWN message
	// returns the object.
	REMOTE_OWN(RequestObjectAccessType),

	// thread a abandons ownership of object b. When send from 
	// broker to a thread c, this means that this thread should 
	// take over ownership of the object. When send from a thread 
	// to broker in response to a RQ_OWN message, the last tuple 
	// element indicates the original asker.
	REMOTE_DISOWN(~JavaObject, uint),
}




pub type ObjectSet = HashMap<JavaObjectId, ~JavaObject>;


// Top-level ObjectBroker message type
pub enum ObjectBrokerMessage {
	// ## Object management ##
	OB_REMOTE_OBJECT_OP(uint, JavaObjectId, RemoteObjectOpMessage),


	// ## Thread management ##
	// a new thread a registers with the object broker
	OB_REGISTER(uint, Chan<ObjectBrokerMessage>),

	// A thread unregisters itself from the object broker,
	// which also abandons the corresponding channel.
	// The message also transmits all remaining objects owned
	// by this thread.
	OB_UNREGISTER(uint, ObjectSet),


	// ## Thread operations ##
	OB_THREAD_REMOTE_OP(uint, uint, RemoteThreadOpMessage),


	// ## VM management ##
	// Connection to VM 
	OB_VM_TO_BROKER(vm::VMToBrokerControlMessage),

	// A thread sends this to broker in response to a System.exit(code)
	// and broker sends this to all threads once it determines that
	// the last non-daemon thread is dead.
	OB_SHUTDOWN(uint, int)
}

#[deriving(Eq)]
enum ShutdownState {
	NOT_IN_SHUTDOWN,
	SHUTTING_DOWN,
	SHUT_DOWN,
}


// The ObjectBroker handles ownership for concurrently accessed
// objects. At every time, every object has one well-defined owner.
// If a thread needs access to an object that it does not currently
// own, it submits a REMOTE_OWN message to the object broker, which
// in turn asks the thread who owns the object to relinquish it using
// a REMOTE_DISOWN message. The owning thread gives up ownership and
// sends a REMOTE_DISOWN message to the broker, which forwards it to
// the original thread and updates its book-keeping to reflect the
// change in ownership.
//
// The ObjectBroker keeps a HM of object ids mapped to their owning
// thread ids.
//
// When a thread dies, it forwards all of its alive objects to the 
// object broker using a REMOTE_DISOWN message. The broker, in turn, 
// keeps those objects internally until another thread demands to
// own them. 
pub struct ObjectBroker {
	// back connection to VM
	priv vm_chan : Chan< vm::BrokerToVMControlMessage>,

	priv threads : ThreadManager,

	priv objects_with_owners: HashMap<JavaObjectId, uint>,
	priv objects_owned : ObjectSet,

	priv in_port : Port<ObjectBrokerMessage>,
	priv thread_chans : HashMap<uint, Chan<ObjectBrokerMessage>>,

	// this is duplicated into all threads
	priv in_shared_chan : SharedChan<ObjectBrokerMessage>,

	// once an REMOTE_OWN message has been sent to a thread,
	// all further requests to the same object are saved 
	// up and dispatched to whomever gains new ownership
	// of the objects. 
	priv waiting_shelf : HashMap<JavaObjectId, ~[ObjectBrokerMessage]>,

	// TODO: how to guarantee object transfer if threads are blocking?


	priv shutdown_state : ShutdownState,
}

static NO_THREAD_INDEX : uint = 0;

static OB_INITIAL_OBJ_HASHMAP_CAPACITY : uint = 4096;
static OB_INITIAL_THREAD_CAPACITY : uint = 16;
static OB_INITIAL_WAITING_SHELF_CAPACITY : uint = 256;


static EXIT_CODE_VM_INITIATED_SHUTDOWN : int = -150392;


impl ObjectBroker {

	// ----------------------------------------------
	pub fn new(cstream : Chan< vm::BrokerToVMControlMessage>) -> ObjectBroker
	{
	 	let (out,input) = SharedChan::new();
		ObjectBroker {
			vm_chan : cstream,
			threads : ThreadManager::new(),

			// maps object-ids (oid) to their other thread-ids (tid) or to 0
			// if the broker owns them (i.e. they are in objects_owned).
			objects_with_owners : HashMap::with_capacity(OB_INITIAL_OBJ_HASHMAP_CAPACITY),
			objects_owned : HashMap::new(),

			//
			in_port : out,
			in_shared_chan : input,

			// maps thread-id to the corresponding channels to send data to
			thread_chans : HashMap::with_capacity(OB_INITIAL_THREAD_CAPACITY),

			// waiting queue for messages sent to objects that are currently
			// being transferred between threads.
			waiting_shelf : HashMap::with_capacity(OB_INITIAL_WAITING_SHELF_CAPACITY),

			shutdown_state : NOT_IN_SHUTDOWN,
		}
	}


	// ----------------------------------------------
	// Launches the object broker. Returns a SharedChan object that can be used
	// to direct messages to the broker. 
	pub fn launch(mut self) -> SharedChan<ObjectBrokerMessage> {
		let ret_chan = self.in_shared_chan.clone();

		// ownership of th ObjectBroker instance moves into the task,
		// all the caller gets back is a channel to communicate.
		do spawn {
			let mut s = self; 
			while s.handle_message() {}
			// die once self goes out of scope 
		}
		return ret_chan;
	}

	// IMPL

	// ----------------------------------------------
	fn handle_message(&mut self) -> bool {
		
		// recv() can not fail because that would mean the SharedChan had 
		// been deallocated, which means the VM ceased to exist. Per 
		// contract it sends vm::VM_TO_BROKER_ACK_SHUTDOWN before it does 
		// so, and after we receive this message we never recv() again.
		match self.in_port.recv() {

			OB_REMOTE_OBJECT_OP(a, b, remote_op) => {
				self.handle_object_op(a, b, remote_op)
			},

			OB_THREAD_REMOTE_OP(a, b, remote_op) => {
				self.threads.process_message(a, b, remote_op)
			},


			OB_VM_TO_BROKER(op) => {
				match op {
					vm::VM_TO_BROKER_DO_SHUTDOWN => 
						self.shutdown_protocol(EXIT_CODE_VM_INITIATED_SHUTDOWN),

					vm::VM_TO_BROKER_ACK_SHUTDOWN => {
						assert_eq!(self.shutdown_state, SHUT_DOWN);

						// trigger self-destruction
						return false;
					}
				}
			},


			OB_SHUTDOWN(a, exit_code) => {
				self.shutdown_protocol(exit_code);
			},


			OB_REGISTER(a, chan) => {
				// no thread may even register with the tid 0 as this
				// is a reserved value.
				assert!(a != 0);

				assert!(!self.thread_chans.contains_key(&a));
				self.thread_chans.insert(a, chan);
				debug!("object broker registered with thread {}", a);

				// also register the thread with the thread manager
				// TODO: GID
				self.threads.add_thread(a, 0);
				assert_eq!(self.threads.get_state(), threadmanager::TMS_Running);
			},


			OB_UNREGISTER(a, in_objects) => {
				assert!(self.thread_chans.contains_key(&a));
				self.thread_chans.pop(&a);

				// own all objects
				for (a,b) in in_objects.move_iter() {
					self.objects_owned.insert(a,b);
					*self.objects_with_owners.get_mut(&a) = 0;
				}

				self.verify_thread_owns_no_objects(a);

				debug!("object broker unregistered with thread {}", a);

				// unregister the thread from threadmanager and check if this
				// was the last non-daemon thread. In this case, we initiate
				// the shutdown sequence with the "success" exit code of 0.
				self.threads.remove_thread(a);
				match self.threads.get_state() {
					threadmanager::TMS_NoThreadSeenYet => fail!("logic error, impossible state"),
					threadmanager::TMS_Running => (),
					threadmanager::TMS_AllNonDaemonsDead => {
						self.shutdown_protocol(0);
					},
				}
			},
		}
		return true;
	}


	#[cfg(debug)]
	#[cfg(test)]
	fn verify_thread_owns_no_objects(&self, a : uint) {
		for (bob, obt) in self.objects_with_owners.iter() {
			assert!(*obt != a);
		}
	}

	#[cfg(release)]
	fn verify_thread_owns_no_objects(&self, a : uint) {
	}


	// ----------------------------------------------
	fn shutdown_protocol(&mut self, exit_code : int) {
		// ignore this if we're already shutting down (regardless if complete or not)
		if self.shutdown_state != NOT_IN_SHUTDOWN {
			return;
		}

		debug!("object broker initiating shutdown protocol with exit code {}",exit_code);
		self.shutdown_state = SHUTTING_DOWN;
		
		// send a shutdown message to all threads, including the one
		// who initiated the shutdown. 
		for (_, chan) in self.thread_chans.iter() {
			chan.send(OB_SHUTDOWN(0, exit_code));
		}

		// and wait for them to unregister
		while self.thread_chans.len() > 0 {
			let res = self.handle_message();
			// this may not cause the objectbroker to destruct itself
			assert_eq!(res, true);
		}

		// notify the VM - it may now send an ACK, causing us
		// to hang up on all connections.
		self.shutdown_state = SHUT_DOWN;
		self.vm_chan.send(vm::BROKER_TO_VM_DID_SHUTDOWN(exit_code));
	}


	// ----------------------------------------------
	fn handle_object_op(&mut self, a : uint, b : JavaObjectId, op : RemoteObjectOpMessage)
	{	
		// check if the object in question is currently being transferred
		// between threads and any further requests are therefore shelved
		// until a new owner is in place
		match op {
			REMOTE_WHO_OWNS => (),
			REMOTE_DISOWN(obj,receiver) => { 
				{	let ref mut objects = self.objects_with_owners;
					let ref mut threads = self.thread_chans;

					// must own object to be able to disown it
					assert!(*objects.get(&b) == a);

					*objects.get_mut(&b) = receiver;
					let t = threads.get(&receiver);
					t.send(OB_REMOTE_OBJECT_OP(a, b, REMOTE_DISOWN(obj, receiver )));
				}

				// cleanup shelf, sending the messages all in the right order,
				// but not more than one OWN message
				let mut sh = self.waiting_shelf.pop(&b).unwrap();
				while sh.len() > 0 {
					match sh.shift() {
						OB_REMOTE_OBJECT_OP(a, b, op) => self.handle_object_op(a, b, op),
						_ => fail!("logic error, cannot shelve this message"),
					}
				}
				return;
			},
			_ => {
				match self.waiting_shelf.find_mut(&b) {
					Some(ref v) => {
						v.push( OB_REMOTE_OBJECT_OP(a,b,op) );
						return;
					},
					_ => ()
				}
			}
		}

		let ref mut objects = self.objects_with_owners;
		let ref mut threads = self.thread_chans;

		match op {
			REMOTE_ADD_REF => {		
				// for somebody to have a reference to a field and thus
				// being able to addref/release it, they must have had
				// accss to an object that was owned by this thread.
				// As this object must have been transferred using the
				// object broker and messages are guaranteed to be 
				// ordered, we must already know about that object.
				
				// Therefore, whether the object is present in the HM
				// is safe for determining whether it is new.
				match objects.find(&b) {
					Some(owner) if *owner == 0 => {
						self.objects_owned.get_mut(&b).intern_add_ref();
					},
					Some(owner) => {
						let t = threads.get(owner);
						t.send(OB_REMOTE_OBJECT_OP(a,b,REMOTE_ADD_REF));
						return;
					},
					_ => (),
				}
				objects.insert(b,a);
			},

			REMOTE_RELEASE => {
				// correctness follows by the same reasoning as for AddRef()
				let owner = *objects.get(&b);
				if owner == a {
					objects.remove(&b);
				}
				else if owner == 0 {
					if !self.objects_owned.get_mut(&b).intern_release() {
						self.objects_owned.pop(&b);
					}
				}
				else {
					let t = threads.get(&owner);
					t.send(OB_REMOTE_OBJECT_OP(a,b,REMOTE_RELEASE));
				}
			},


			REMOTE_WHO_OWNS => {
				let t = threads.get(&a);
				t.send(OB_REMOTE_OBJECT_OP(*objects.get(&b),b,REMOTE_WHO_OWNS));
			},


			REMOTE_OWN(rmode) => {
				// b must be in objects as per the same reasoning as 
				// OB_RQ_ADD_REF() is sound.

				let owner = *objects.get(&b);
				// cannot request object oned by oneself
				// bookkeeping of own owned objects is consistent,
				// so failure to hold this would be a logic error.
				assert!(owner != a);

				// if the broker owns this object, send it immediately
				if owner == 0 {
					*objects.get_mut(&b) = a;

					let op = REMOTE_DISOWN(self.objects_owned.pop(&b).unwrap(), a);
					let t = threads.get(&a);
					t.send(OB_REMOTE_OBJECT_OP(0, b, op));
					return;
				}

				let t = threads.get(&owner);
				t.send(OB_REMOTE_OBJECT_OP(a, b, REMOTE_OWN(rmode)));

				// from now on, shelve any further requests pertaining
				// to this object until the new owner has taken over.
				self.waiting_shelf.insert(b, ~[]);
			},

			REMOTE_DISOWN(obj,receiver) => fail!("logic error, handled earlier"),
		}
	}
}

#[cfg(test)]
mod tests {
	use objectbroker::*;
	use vm;

	use object::{JavaObject};
	use std::task::{task};
	use std::hashmap::{HashMap};

	use classloader::tests::{test_get_real_classloader};

	type test_proc = proc(&SharedChan<ObjectBrokerMessage>, Port<ObjectBrokerMessage>) -> ();

	// ----------------------------------------------
	fn test_setup(a : test_proc, b : test_proc, expect_success_exit_code : bool) {
		let (port, chan) = Chan::new();
		let mut ob = ObjectBroker::new(chan);
		let chan = ob.launch();

		// thread 1
		let input1 = chan.clone();
		let mut t1 = task();
		let f1 = t1.future_result();
		do t1.spawn {
			let (port, chan) = Chan::new();
			input1.send(OB_REGISTER(1, chan));

			a(&input1, port);
			// ensure proper cleanup - without the objectbroker would fail on the hung up channel
			input1.send(OB_UNREGISTER(1, HashMap::new()));
		}

		// thread 2
		let input2 = chan.clone();
		let mut t2 = task();
		let f2 = t2.future_result();
		do t2.spawn {
			let (port, chan) = Chan::new();
			input2.send(OB_REGISTER(2, chan));

			b(&input2, port);
			input2.send(OB_UNREGISTER(2, HashMap::new()));
		}

		f1.recv();
		f2.recv();

		// at this point, both threads unregistered and the VM is therefore
		// supposed to shut down because no non-daemon thread is alive.
		// this gives an exit code of 0
		match port.recv() {
			vm::BROKER_TO_VM_DID_SHUTDOWN(exit_code) 
				if exit_code == 0 || !expect_success_exit_code => (),

			_ => assert!(false),
		}

		chan.send(OB_VM_TO_BROKER(vm::VM_TO_BROKER_ACK_SHUTDOWN)); 
	}


	// ----------------------------------------------
	// Shutdown initiated by VM - VM::exit() called
	#[test]
	fn test_shutdown_initiated_by_vm() {
		let (port, chan) = Chan::new();
		let mut ob = ObjectBroker::new(chan);
		let chan = ob.launch();

		chan.send(OB_VM_TO_BROKER(vm::VM_TO_BROKER_DO_SHUTDOWN));

		// must confirm the shutdown with a negative exit code
		match port.recv() {
			vm::BROKER_TO_VM_DID_SHUTDOWN(EXIT_CODE_VM_INITIATED_SHUTDOWN) => (),
			_ => assert!(false),
		}

		// after we ack that we received the shutdown confirmation, the connection
		// should die - but not earlier.
		chan.send(OB_VM_TO_BROKER(vm::VM_TO_BROKER_ACK_SHUTDOWN));
	}



	// ----------------------------------------------
	// Shutdown initiated by thread - i.e. System.exit() called
	#[test]
	fn test_shutdown_initiated_by_threads() {
		let mut cl = test_get_real_classloader();
		let v = cl.add_from_classfile("EmptyClass").unwrap_all();

		let (sync_port, sync_chan) = Chan::new();
		let (sync_port2, sync_chan2) = Chan::new();

		test_setup(
			proc(input : &SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
				sync_port2.recv();
				input.send(OB_SHUTDOWN(1,15));
				sync_chan.send(1);

				// even the initiating thread gets a message
				let request = output.recv();
				match request {
					OB_SHUTDOWN(0,15) => (),
					_ => assert!(false),
				}
			},
			proc(input : &SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
				sync_chan2.send(1);
				sync_port.recv();
				input.send(OB_SHUTDOWN(2,16));

				// the first exit code wins so the exit code cannot be 16
				let request = output.recv();
				match request {
					OB_SHUTDOWN(0,15) => (),
					_ => assert!(false),
				}
			}
		, false);
	}


	// ----------------------------------------------
	// Object transfers semantics and regular shutdown caused by all
	// non-daemon threads having died.
	#[test]
	fn test_object_broker() {
		let mut cl = test_get_real_classloader();
		let v = cl.add_from_classfile("EmptyClass").unwrap_all();

		let (sync_port, sync_chan) = Chan::new();
		test_setup(
			proc(input : &SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
				// register object 15
				input.send(OB_REMOTE_OBJECT_OP(1,15,REMOTE_ADD_REF));
				sync_chan.send(1);

				let request = output.recv();
				match request {
					OB_REMOTE_OBJECT_OP(2,15,REMOTE_OWN (mode) ) => {
						assert_eq!(mode, OBJECT_ACCESS_Normal);
						input.send(OB_REMOTE_OBJECT_OP(1,15,REMOTE_DISOWN(~JavaObject::new(*v,0),2)))
					},
					_ => assert!(false),
				}
			},
			proc(input : &SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
				// we have to ensure that object 15 is registered. Normally,
				// this is implicitly guaranteed because how would we otherwise
				// know about its id?
				sync_port.recv();

				// want to own object 15
				input.send(OB_REMOTE_OBJECT_OP(2,15,REMOTE_OWN(OBJECT_ACCESS_Normal) ));

				let response = output.recv();
				match response {
					OB_REMOTE_OBJECT_OP(1,15,REMOTE_DISOWN(val,2)) => {
						let cl = val.get_class();
						assert_eq!(*cl.get().get_name(), ~"EmptyClass");
					},
					_ => assert!(false),
				}

				// release the object. Otherwise we will assert upon unregistering
				// this thread given that it still owns an object.
				input.send(OB_REMOTE_OBJECT_OP(2,15,REMOTE_RELEASE));
			}
		, true);
	}
}

// TODO: tests of more complex scenarios
