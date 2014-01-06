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


