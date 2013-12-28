

// --------------------------------------------------------------------
// Assert that given `Result< ???, ~str>` is not an error, otherwise
// print the error message attached to it.
pub fn assert_no_err<T> (given : &Result<T, ~str>) {
	match *given {
		Err(ref s) => fail!("expected no error, error is: {}", s.clone()),
		_ => ()
	}
}

pub fn assert_is_err<T, S> (given : &Result<T, S>) {
	match *given {
		Ok(ref s) => fail!("expected  error, but no error occured"),
		_ => ()
	}
}


