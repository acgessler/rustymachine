

// --------------------------------------------------------------------
// Assert that given `Result< ???, ~str>` is not an error, otherwise
// print the error message attached to it.
pub fn assert_no_err<T> (given : Result<T, ~str>) {
	match given {
		Err(s) => fail!("expected no error, error is: {}", s),
		_ => ()
	}
}

