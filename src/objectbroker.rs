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

use object::{JavaObject, JavaObjectId};

use threadmanager::{ThreadManager, RemoteThreadOpMessage};



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
	// Thread sends this to broker in response to a System.exit(code)
	// and broker sends this to all threads once it determines that
	// the last non-daemon thread is dead.
	OB_SHUTDOWN(uint, int)
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
	priv threads : ThreadManager,

	priv objects_with_owners: HashMap<JavaObjectId, uint>,
	priv objects_owned : ObjectSet,

	priv in_port : Port<ObjectBrokerMessage>,
	priv out_chan : HashMap<uint, Chan<ObjectBrokerMessage>>,

	// this is duplicated into all threads
	priv in_shared_chan : SharedChan<ObjectBrokerMessage>,

	// once an REMOTE_OWN message has been sent to a thread,
	// all further requests to the same object are saved 
	// up and dispatched to whomever gains new ownership
	// of the objects. 
	priv waiting_shelf : HashMap<JavaObjectId, ~[ObjectBrokerMessage]>

	// TODO: how to guarantee object transfer if threads are blocking?
}

static NO_THREAD_INDEX : uint = 0;

static OB_INITIAL_OBJ_HASHMAP_CAPACITY : uint = 4096;
static OB_INITIAL_THREAD_CAPACITY : uint = 16;
static OB_INITIAL_WAITING_SHELF_CAPACITY : uint = 256;

impl ObjectBroker {

	// ----------------------------------------------
	pub fn new() -> ObjectBroker
	{
	 	let (out,input) = SharedChan::new();
		ObjectBroker {
			threads : ThreadManager::new(),
			objects_with_owners : HashMap::with_capacity(OB_INITIAL_OBJ_HASHMAP_CAPACITY),
			objects_owned : HashMap::new(),

			in_port : out,
			in_shared_chan : input,
			out_chan : HashMap::with_capacity(OB_INITIAL_THREAD_CAPACITY),
			waiting_shelf : HashMap::with_capacity(OB_INITIAL_WAITING_SHELF_CAPACITY)
		}
	}


	// ----------------------------------------------
	// Launches the object broker. Returns a SharedChan object that can be used
	// to direct messages to the broker. Send an OB_SHUTDOWN to terminate
	// operation.
	pub fn launch(mut self) -> SharedChan<ObjectBrokerMessage> {
		let ret_chan = self.in_shared_chan.clone();

		// ownership of th ObjectBroker instance moves into the task,
		// all the caller gets back is a channel to communicate.
		do spawn {
			let mut s = self; 
			while s.handle_message() {}
		}
		return ret_chan;
	}

	// IMPL

	// ----------------------------------------------
	fn handle_message(&mut self) -> bool {
		match self.in_port.recv() {

			OB_REMOTE_OBJECT_OP(a, b, remote_op) => {
				self.handle_object_op(a, b, remote_op)
			},

			OB_THREAD_REMOTE_OP(a, b, remote_op) => {
				self.threads.process_message(a, b, remote_op)
			}


			OB_REGISTER(a, chan) => {
				let ref mut threads = self.out_chan;
				assert!(!threads.contains_key(&a));
				threads.insert(a, chan);
				debug!("object broker registered with thread {}", a);
			},


			OB_UNREGISTER(a, objects) => {
				let ref mut threads = self.out_chan;
				assert!(threads.contains_key(&a));
				threads.pop(&a);

				// own all objects
				for (a,b) in objects.move_iter() {
					self.objects_owned.insert(a,b);
					*self.objects.get_mut(a) = 0;
				}

				debug!("object broker unregistered with thread {}", a);
			},


			OB_SHUTDOWN(a, exit_code) => {
				debug!("object broker shutting down with exit code {}",exit_code);
				return false;
			},
		}
		return true;
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
					let ref mut threads = self.out_chan;

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
		let ref mut threads = self.out_chan;

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
					Some(owner) if owner == 0 => {
						self.objects_owned.get(&b).add_ref();
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
					self.objects_owned.get(&b).release();
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
					let op = REMOTE_DISOWN(self.objects_owned.pop().unwrap(), a);
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

	use object::{JavaObject};
	use std::task::{task};

	use classloader::tests::{test_get_real_classloader};

	type test_proc = proc(SharedChan<ObjectBrokerMessage>, Port<ObjectBrokerMessage>) -> ();

	fn test_setup(a : test_proc, b : test_proc) {
		let mut ob = ObjectBroker::new();
		let chan = ob.launch();

		// thread 1
		let input1 = chan.clone();
		let mut t1 = task();
		let f1 = t1.future_result();
		do t1.spawn {
			let (port, chan) = Chan::new();
			input1.send(OB_REGISTER(1, chan));

			a(input1, port);
		}

		// thread 2
		let input2 = chan.clone();
		let mut t2 = task();
		let f2 = t2.future_result();
		do t2.spawn {
			let (port, chan) = Chan::new();
			input2.send(OB_REGISTER(2, chan));

			b(input2, port);
		}

		f1.recv();
		f2.recv();
	}


	#[test]
	fn test_object_broker() {
		let mut cl = test_get_real_classloader();
		let v = cl.add_from_classfile("EmptyClass").unwrap_all();

		let (sync_port, sync_chan) = Chan::new();
		test_setup(
			proc(input : SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
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
			proc(input : SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
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
						input.send(OB_SHUTDOWN(1,-1));
					},
					_ => assert!(false),
				}
			}
		);
	}
}

// TODO: tests of more complex scenarios
