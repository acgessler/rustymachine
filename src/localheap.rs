
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

use std::hashmap::{HashMap};

use std::sync::atomics::{atomic_add, AcqRel};

use std::ptr;

use thread::{ThreadContext};
use object::{JavaObject, JavaObjectId};
use class::{JavaClassRef};
use objectbroker::*;


// LocalHeap is a thread-local utility for threads to create,
// destroy and access Java objects. Even though it is technically
// not a heap (actual heap management is forwarded to Rust's
// runtime heap manager), it is referred to as such because of it
// behaviour which is to provide the Java heap.
//
// LocalHeap is tightly 1:1 coupled to a ThreadContext. 
// ThreadContext forwards all OB_RQ messages that it receives from 
// ObjectBroker to LocalHeap.


pub struct LocalHeap {
	// backref to owning thread. Unfortunately a borrowed ref
	// cannot solve this so we need an unsafe pointer.
	// http://stackoverflow.com/questions/20698384
	priv thread : *mut ThreadContext,

	// shortcut to thread-id
	priv tid : uint,
	
	// heap objects currently owned by this thread context
	priv owned_objects : HashMap<JavaObjectId, ~JavaObject>,

	// 
}

static LH_INITIAL_OBJ_HASHMAP_CAPACITY : uint = 1024;
static mut ObjectIdCounter : JavaObjectId = 0;

impl LocalHeap  {

	// ----------------------------------------------
	pub fn dummy() -> LocalHeap {
		LocalHeap {
			thread : ptr::mut_null(),
			tid : 0,
			owned_objects : HashMap::new(),
		}
	}


	// ----------------------------------------------
	pub unsafe fn new_with_owner(t : &mut ThreadContext) -> LocalHeap {
		LocalHeap {
			thread : ptr::to_mut_unsafe_ptr(t),
			tid : t.get_tid(),

			owned_objects : HashMap::with_capacity(LH_INITIAL_OBJ_HASHMAP_CAPACITY),
		}
	}


	// ----------------------------------------------
	#[inline]
	fn get_thread<'t>(&'t self) -> &'t ThreadContext {
		unsafe { &*self.thread }
	}

	#[inline]
	fn get_thread_mut<'t>(&'t self) -> &'t mut ThreadContext {
		unsafe { &mut *self.thread }
	}


	// ----------------------------------------------
	pub fn new_object(&mut self, jclass : JavaClassRef) -> JavaObjectId {
		// generate an unique object id
		let id = unsafe {
			atomic_add(&mut ObjectIdCounter, 1, AcqRel)
		};

		// this id must be unique - if not, we ran out of
		// 64bit indices ("impossible - our shields cannot be 
		// broken") or there is a logic flaw somewhere.
		assert!(!self.owned_objects.contains_key(&id));

		// tell the object broker to ensure other threads
		// can request the object by its oid
		let op = OB_REMOTE_OBJECT_OP(self.tid, id,REMOTE_ADD_REF);

		self.get_thread_mut().send_message(op);
		self.owned_objects.insert(id, ~JavaObject::new(jclass, id));
		id
	}


	// ----------------------------------------------
	pub fn new_array_object() {
		// TODO
	}


	// ----------------------------------------------
	// AddRef a specific java object. This works both for local
	// objects (i.e. owned by current thread) and for remote
	// objects.
	pub fn add_ref(&mut self, oid : JavaObjectId) {
		// if this is a local object, addref it
		match self.owned_objects.find_mut(&oid) {
			Some(obj) => {
				obj.intern_add_ref();
				return
			},
			// fallthru
			None => () 
		}
		// forward request to ObjectBroker for remote objects
		let op = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_ADD_REF);
		self.get_thread().send_message(op);
	}


	// ----------------------------------------------
	// AddRef a specific java object. This works both for local
	// objects (i.e. owned by current thread) and for remote
	// objects.
	pub fn release(&mut self, oid : JavaObjectId) {
		// forward request to ObjectBroker for remote objects
		if !self.owned_objects.contains_key(&oid) {
			let op = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_RELEASE);
			self.get_thread().send_message(op);
			return;
		}

		{
			// if this is a local object, release it 
			let m = self.owned_objects.find_mut(&oid).unwrap();
			if m.intern_release() {
				return;
			}
		}
		
		// the object's reference counter reached zero
		// and we can therefore safely drop it.
		self.owned_objects.pop(&oid);
	}


	// ----------------------------------------------
	// Access a specific object. If the object requested is owned (or 
	// locked, depending on the access mode requested) by the current 
	// thread, access is immediately granted, otherwise the current 
	// task blocks until ownership can be obtained.
	//
	// The caller is required to ensure that the object requested remains
	// alive until control returns. This can be achieved e.g. by holding
	// a strong reference to it.
	//
	// The `access` parameter specifies the kind of access requested on 
	// the object. Note that OBJECT_ACCESS_NORMAL is always granted unless
	// the thread who currently owns that object is deadlocked and any
	// of the MONITOR_ access modes can be a cause of deadlock.
	//
	// TODO: how do we deal with deadlocks in general?
	//
	// The closure passed in is called exactly once with a borrowed ref to
	// the object, to which it gets full access but cannot dispose of
	pub fn access_object(&mut self, access : RequestObjectAccessType, 
		oid : JavaObjectId, wrap : |&JavaObject| -> ()) 
	{
		let mut done = false;
		let mut send_to_thread : Option<uint> = None;
		match self.owned_objects.find_mut(&oid) {
			Some(ref obj) => {

				match access {

					OBJECT_ACCESS_Normal => {
						wrap(**obj);
						done = true;
					},
					OBJECT_ACCESS_Monitor | OBJECT_ACCESS_MonitorPriority 
						// even if we own the object, somebody else could
						// have the monitor lock.
						if obj.monitor().can_be_locked_by_thread(self.tid) => {
							wrap(**obj);
							done = true;
					},

					// fallthru
					_ => ()
				}

				if done {
					send_to_thread = obj.monitor_mut().pop_ready_thread();
				}
			},
			// fallthru
			None => () 
		}

		// check if we have any pending waiters for the object's monitor,
		// if so, satisfy them immediately. If we did this in
		// handle_pending_messages() [which might make more sense
		// in terms of code hygiene], we would need to maintain shortlist 
		// of available objects.
		if done {
			match send_to_thread {
				None => (),
				Some(tid) => {
					self.send_to_thread(oid, tid);
				}
			} 
			return;
		} 

		// request to own the object
		let op = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_OWN(access));
		self.get_thread().send_message(op);

		// and block until we can get it
		if self.get_thread_mut().handle_messages_until(|msg : &ObjectBrokerMessage| {
			match *msg {
				OB_REMOTE_OBJECT_OP(ref rtid, ref roid, REMOTE_DISOWN(ref obj, ref rec)) => {
					// when waiting for objects, we always block on
					// obtaining them so it is not possible that 
					// multiple requests are sent and responses
					// received in a different order.
					assert_eq!(*rec, self.tid);

					// also verify that the access mode requirement is fullfilled
					assert!(access != OBJECT_ACCESS_Monitor || 
						    access != OBJECT_ACCESS_MonitorPriority || 
						    obj.monitor().can_be_locked_by_thread(self.tid)
					); 
					true
				},
				_ => false
			}
		}) {
			self.access_object(access, oid, wrap)
		} // else: VM shutdown - we simply ignore the closure
	}


	// ----------------------------------------------
	// Transfer ownership of an object to a particular thread
	pub fn send_to_thread(&mut self, oid : JavaObjectId, tid : uint) {
		let obj = self.owned_objects.pop(&oid).unwrap();
		let m = OB_REMOTE_OBJECT_OP(self.tid, oid, REMOTE_DISOWN(obj, tid));
		self.get_thread().send_message(m);
	}



	// ----------------------------------------------
	// Transfers ownership of the hashmap containing all owned
	// objects to the caller and destroys the LocalHeap
	pub fn unwrap_objects(self) -> HashMap<JavaObjectId,~JavaObject> {
		self.owned_objects
	}


	// ----------------------------------------------
	// Check if a particular object is currently owned by this thread
	pub fn owns(&self, b : JavaObjectId) -> bool {
		return self.owned_objects.contains_key(&b);
	}


	// ----------------------------------------------
	// Handle any of the remote object messages 
	// a is the source thread id, and b is the object in question.
	pub fn handle_message(&mut self, a : uint, b : JavaObjectId, op : RemoteObjectOpMessage) {
		// TODO: owns() is not necessarily satisfied if we send back objects without being asked for
		assert!(self.owns(b));
		match op {
			REMOTE_WHO_OWNS => fail!("logic error, WHO_OWNS is not handled by threads"),
			REMOTE_ADD_REF => self.add_ref(b),
			REMOTE_RELEASE => self.release(b),
			REMOTE_OWN(mode) => {
				match mode {
					OBJECT_ACCESS_Monitor | OBJECT_ACCESS_MonitorPriority => {
						let obj = self.owned_objects.get_mut(&b);

						// we should assume that, in order to request Priority access,
						// the sender thread should already own the monitor as is
						// the requirement for calling wait() on an object.
						assert!(mode != OBJECT_ACCESS_MonitorPriority || 
							obj.monitor().is_locked_by_thread(a));

						if !obj.monitor().can_be_locked_by_thread(a) {
							// append the thread to the monitor's waiting queues
							obj.monitor_mut().push_thread(a, 
								mode == OBJECT_ACCESS_MonitorPriority
							);

							return;
						}
					},
					// fallthru
					_ => (),
				}
				self.send_to_thread(b, a);
			},
			
			REMOTE_DISOWN(obj,rec) => {
				// currently we should not be receiving objects that we
				// did not request using OB_RQ_OWN
				assert_eq!(rec, self.tid);
				self.owned_objects.insert(b, obj);
			},
		}
	}
}



// A JavaStrongObjectRef is a reference to a Java object that guarantees
// that the referenced objects stays alive for at least the lifetime of
// the reference. 

pub struct JavaStrongObjectRef 
{
	priv jid : JavaObjectId,
	priv heapref : *mut LocalHeap,
}


impl JavaStrongObjectRef {

	// ----------------------------------------------
	// Construct a strong ref for the given object id
	// note: this triggers a remote add-ref on that object. If the
	// caller's intent is to access the object immediately anyway,
	// it is better not to construct a strong ref but to use
	// heap::access_object().
	//
	// It is assumed that the given localheap remains alive for the entire
	// duration of the object reference. 
	pub fn new(jid : JavaObjectId, heap : &mut LocalHeap) -> JavaStrongObjectRef {
		heap.add_ref(jid);
		JavaStrongObjectRef {
			jid : jid,
			heapref : unsafe { heap }
		}
	}
}


impl Drop for JavaStrongObjectRef {
	fn drop(&mut self) {
		let ref mut heap = unsafe { &mut (*self.heapref) };
		heap.release(self.jid);
	}
}
