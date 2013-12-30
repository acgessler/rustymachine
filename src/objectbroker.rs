
use std::comm::{Chan, Port, SharedChan};

use std::hashmap::{HashMap};

use std::task::{task};

use object::{JavaObject};


pub enum ObjectBrokerMessage {

	// ## Object management ##

	// request from thread a to addref object b. If the object 
	// is not known to the broker yet, this registers the object 
	// as being owned by thread a.
	OB_RQ_ADD_REF(uint, uint),

	// request from thread a to release object b. If thread a 
	// owns this object, the message informs the broker that the
	//  object has been deleted.
	OB_RQ_RELEASE(uint, uint),

	// thread a asks which thread owns object b. Response is the 
	// same message with a (c, b) tuple, c is the id of the the 
	// owning thread.
	OB_RQ_WHO_OWNS(uint, uint),

	// thread a asks to take over ownership of object b
	OB_RQ_OWN(uint, uint),

	// thread a abandons ownership of object b. When send from 
	// broker to a thread c, this means that this thread should 
	// take over ownership of the object. When send from a thread 
	// to broker in response to a RQ_OWN message, the last tuple 
	// element indicates the original asker.
	OB_RQ_DISOWN(uint, uint, JavaObject, uint),


	// ## Thread management ##

	// a new thread a registers with the object broker
	OB_REGISTER(uint, Chan<ObjectBrokerMessage>),


	// ## VM management ##
	OB_SHUTDOWN
}



// The ObjectBroker handles ownership for concurrently accessed
// objects. At every time, every object has one well-defined owner.
// If a thread needs access to an object that it does not currently
// own, it submits a OB_RQ_OWN message to the object broker, which
// in turn asks the thread who owns the object to relinquish it using
// a OB_RQ_DISOWN message. The owning thread gives up ownership and
// sends a OB_RQ_DISOWN message to the broker, which forwards it to
// the original thread and updates its book-keeping to reflect the
// change in ownership.
//
// The ObjectBroker keeps a HM of object ids mapped to their owning
// thread ids.
//
// When a thread dies, it forwards all of its alive objects to the 
// object broker using a OB_OUT_RE_OBJ message. The broker, in turn, 
// keeps those objects internally until another thread demands to
// own them.
pub struct ObjectBroker {
	objects_with_owners: HashMap<uint, uint>,

	in_port : Port<ObjectBrokerMessage>,
	out_chan : HashMap<uint, Chan<ObjectBrokerMessage>>,

	// this is duplicated into all
	in_shared_chan : SharedChan<ObjectBrokerMessage>,
}


static OB_INITIAL_OBJ_HASHMAP_CAPACITY : uint = 4096;
static OB_INITIAL_THREAD_CAPACITY : uint = 16;

impl ObjectBroker {

	// ----------------------------------------------
	pub fn new() -> ObjectBroker
	{
	 	let (out,input) = SharedChan::new();
		ObjectBroker {
			objects_with_owners : HashMap::with_capacity(OB_INITIAL_OBJ_HASHMAP_CAPACITY),

			in_port : out,
			in_shared_chan : input,
			out_chan : HashMap::with_capacity(OB_INITIAL_THREAD_CAPACITY),
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
		let ref mut objects = self.objects_with_owners;
		let ref mut threads = self.out_chan;

		match self.in_port.recv() {
			OB_RQ_ADD_REF(a,b) => {
				// for somebody to have a reference to a field and thus
				// being able to addref/release it, they must have had
				// accss to an object that was owned by this thread.
				// As this object must have been transferred using the
				// object broker and messages are guaranteed to be 
				// ordered, we must already know about that object.
				
				// Therefore, whether the object is present in the HM
				// is safe for determining whether it is new.
				match objects.find(&b) {
					Some(owner) => {
						let t = threads.get(owner);
						t.send(OB_RQ_ADD_REF(a,b));
						return true;
					},
					_ => (),
				}
				objects.insert(b,a);
			}

			OB_RQ_RELEASE(a,b) => {
				// correctness follows by the same reasoning as for AddRef()
				let owner = *objects.get(&b);
				if owner == a {
					objects.remove(&b);
				}
				else {
					let t = threads.get(&owner);
					t.send(OB_RQ_RELEASE(a,b));
				}
			},


			OB_RQ_WHO_OWNS(a,b) => {
				let t = threads.get(&a);
				t.send(OB_RQ_WHO_OWNS(*objects.get(&b),b));
			},


			OB_RQ_OWN(a,b) => {
				// b must be in objects as per the same reasoning as 
				// OB_RQ_ADD_REF() is sound.

				let owner = *objects.get(&b);
				// cannot request object oned by oneself
				// bookkeeping of own owned objects is consistent,
				// so failure to hold this would be a logic error.
				assert!(owner != a);

				// TODO: what if somebody else concurrently requested
				// owning the object, but has not received it yet?

				let t = threads.get(&owner);
				t.send(OB_RQ_OWN(a, b));
			},


			OB_RQ_DISOWN(a,b,obj,receiver) => {
				// must own object to be able to disown it
				assert!(*objects.get(&b) == a);

				*objects.get_mut(&b) = receiver;
				let t = threads.get(&receiver);
				t.send(OB_RQ_DISOWN(a, b, obj, receiver));
			},


			OB_REGISTER(a, chan) => {
				assert!(!threads.contains_key(&a));
				threads.insert(a, chan);
				debug!("object broker registered with thread {}", a);
			},


			OB_SHUTDOWN => {
				debug!("object broker shutting down");
				return false;
			},
		}
		return true;
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
				input.send(OB_RQ_ADD_REF(1,15));
				sync_chan.send(1);

				let request = output.recv();
				match request {
					OB_RQ_OWN(2,15) => input.send(OB_RQ_DISOWN(1,15,JavaObject::new(*v),2)),
					_ => assert!(false),
				}
			},
			proc(input : SharedChan<ObjectBrokerMessage>, output: Port<ObjectBrokerMessage>) {
				// we have to ensure that object 15 is registered. Normally,
				// this is implicitly guaranteed because how would we otherwise
				// know about its id?
				sync_port.recv();

				// want to own object 15
				input.send(OB_RQ_OWN(2,15));

				let response = output.recv();
				match response {
					OB_RQ_DISOWN(1,15,val,2) => {
						let cl = val.get_class();
						assert_eq!(*cl.get().get_name(), ~"EmptyClass");
						input.send(OB_SHUTDOWN);
					},
					_ => assert!(false),
				}
			}
		);
	}
}

// TODO: tests of more complex scenarios
