
use objectbroker::*;
use vm::{ThreadContext};
use localheap::{LocalHeap};

// Implementation of a basic Java monitor object. Monitors as
// mandated by Java generally have weaker properties than the 
// original theoretical concept by C.A.R Hoare and Hansen.
// 
// Differences include:
//
// - There is only one, implicit condition variable - 
//   the monitor itself. This can be worked around by using
//   multiple monitors though.
// - The wait()ing threads are not necessarily woken up in
//   the order in which they called wait().
// - wait()ing threads have no priority over threads that
//   regularly enter the mutex (from outside).
// 
// Another, more implementation-related difference is the concept
// of "spurious wakeups" which means that wait() on a mutex
// sometimes returns without the condition waited for being
// fullfilled. Therefore, proper use of wait() always includes
// re-checking the original condition:
//
// while (!condition) { obj.wait() }
//
// Note that, given spurious wakeups, notify() could essentially
// be implemented through notifyAll().
//
// **** THIS IMPLEMENTATION ****
//
// We implement the original monitor concept since it is (to my
// understanding) a strict subset of Java's monitor semantics.
//
// Given that the ObjectBroker implicitly serializes access to
// objects, it is relatively easy to guarantee a certain order
// in which threads get to access a monitor.


//
//
// In the context of this VM's terminology, care must be taken to 
// disambiguate between owning objects (which is required even for
// plain access to fields) and owning the monitor for an object 
// which guarantees atomicity across multiple operations.

pub struct JavaMonitor {

	// If the owning thread context currently holds a monitor
	// (i.e. current opcode is within at least one 
	//  mutexenter ... mutexleave block), this is a positive
	// value. We use an integer count to allow recursive use.
	priv lock_count : uint,

	// Thread-id that currently owns the monitor. 
	// This value is None() iff lock_count == 0
	priv owner : Option<uint>,


	// Waiting queue for the monitor. Each entry is a thread
	// id indicating the thread requesting to enter the 
	// monitor.
	priv waiters : ~[uint],

	// Priority waiting queue for the object. Threads that
	// wait() on an object are considered priority waiters.
	// For each thread there is also a boolean specifying 
	// whether the waiter has been notified or not and the
	// list is monotonously decreasing with regard to this
	// boolean, i.e. if one element is not notified, all the 
	// elements behind in the list are neither.
	//
	// The third tuple element is the value of the mutex
	// counter at the time wait() was called. Once a waiting
	// thread is notified and resumes operation, it owns the
	// mutex again with the very same lock_count.
	priv waiters_prio : ~[(bool, uint, uint)],
}


impl JavaMonitor {

	// ----------------------------------------------
	pub fn new() -> JavaMonitor 
	{
		JavaMonitor {
			lock_count : 0,
			owner : None,

			waiters : ~[],
			waiters_prio : ~[],
		}
	}


	// ----------------------------------------------
	pub fn pop_ready_thread(&mut self) -> Option<uint> {
		// no shelved thread can run if the monitor is locked
		if self.is_locked() {
			return None;
		}

		// check if there is any wait()ing thread that has been
		// notify()ed and is therefore ready to run again.
		if self.waiters_prio.len() > 0 {
			let (notified, tid, lock_count) = self.waiters_prio[0];
			if notified {
				self.waiters_prio.shift();
				return Some(tid);
			}
		}

		// otherwise just pick any thread who is waiting to
		// lock the mutex.
		self.waiters.shift_opt()
	}


	// ----------------------------------------------
	// Wait and unlock until another thread calls notify_{one,all} 
	// and atomically lock the mutex again.
	//
	// The monitor must be locked by the current thread.
	#[inline]
	pub fn wait(&mut self, thread : &mut ThreadContext) {
		// assure we hold the monitor
		assert!(self.is_locked_by_thread(thread.get_tid()));
		let tid = thread.get_tid();

		// append the given thread to the end of the list, i.e.
		// this thread gets served last.
		self.waiters_prio.push((false,tid, self.lock_count));

		// TODO: how to actually block now?
	} 


	// ----------------------------------------------
	// Notify one wait()ing thread, if any. The corresponding
	// thread is unblocked and resumes operation. It automatically
	// locks the mutex again.
	//
	// The monitor must be locked by the current thread.
	pub fn notify_one(&mut self, thread : &ThreadContext) {
		// assure we hold the monitor
		assert!(self.is_locked_by_thread(thread.get_tid()));
		
		let mut i = 0;
		let len = self.waiters.len();

		while i < len {
			match self.waiters_prio[i] {
				(false, tid, lock_count) => {
					self.waiters_prio[i] = (true, tid, lock_count);
					return;
				},

				_ => ()
			}
			i += 1;
		}
	}


	// ----------------------------------------------
	// Unlike notify_one(), this marks all wait()ing threads as
	// ready to run again.
	//
	// The monitor must be locked by the current thread.
	pub fn notify_all(&mut self, thread : &ThreadContext) {
		// assure we hold the monitor
		assert!(self.is_locked_by_thread(thread.get_tid()));

		let mut i = 0;
		let len = self.waiters.len();
		
		while i < len {
			match self.waiters_prio[i] {
				(notified, tid, lock_count) => {
					self.waiters_prio[i] = (true, tid, lock_count);
				}
			}
			i += 1;
		}
	}


	// ----------------------------------------------
	// Locks the monitor. Fails if the monitor cannot currently
	// be entered as another thread has it already locked.
	// To block on a monitor until availability, use 
	// LocalHeap::access_object with ACCESS_OBJECT_Monitor
	//
	// Once a monitor has been entered by a thread, the monitor is
	// said to be "locked" by that thread.
	//
	// Recursive calls to lock()/unlock() are supported.
	#[inline]
	pub fn lock(&mut self, thread : &ThreadContext) {
		let tid = thread.get_tid();
		if !self.is_locked_by_thread(tid){
			fail!("cannot lock object");
		}
		self.inc_lock();
		self.owner = Some(tid);
	}


	// ----------------------------------------------
	// Leave the monitor again and thus make it available to 
	// other threads. Every call to lock() must be matched with a
	// call to unlock().
	#[inline]
	pub fn unlock(&mut self, oid : uint, heap : &mut LocalHeap) {
		self.dec_lock();
		// TODO: append wait()ers from mutex to the object waiter list
	}


	// ----------------------------------------------
	// Check if the monitor is currently locked by the given thread
	#[inline]
	pub fn is_locked_by_thread(&self, tid : uint) -> bool {
		return self.lock_count > 0 && self.owner.unwrap() == tid;
	}


	// ----------------------------------------------
	// Check if the monitor is currently locked by any thread.
	#[inline]
	pub fn is_locked(&self) -> bool {
		assert_eq!(self.lock_count > 0, self.owner.is_some());
		self.lock_count > 0
	}


	// ----------------------------------------------
	// Check if the monitor can currently be loked by the given
	// thread. Since lock() is recursively usable, this also
	// returns true if the thread already locks the mutex.
	#[inline]
	pub fn can_be_locked_by_thread(&self, tid : uint) -> bool {
		self.lock_count == 0 || self.owner.unwrap() == tid
	}



	// ----------------------------------------------
	#[inline]
	fn inc_lock(&mut self) {
		self.lock_count += 1;
	}

	#[inline]
	fn dec_lock(&mut self) {
		assert!(self.lock_count > 0);
		self.lock_count -= 1;
	}
}

