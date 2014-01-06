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

use class::{JavaClassFutureRef};


pub struct ExceptionHandler
{
	start_pc : uint,
	end_pc : uint,
	handler_pc : uint,
	catch_type : ~str,
}


pub struct CodeBlock
{
	priv max_stack : uint,
	priv max_locals : uint,
	priv code : ~[u8],
	priv exceptions : ~[ExceptionHandler]
}


impl CodeBlock
{
	// ----------------------------------------------
	pub fn new(max_stack : uint, max_locals : uint, code : ~[u8], exceptions : ~[ExceptionHandler]) -> 
		CodeBlock
	{
		CodeBlock {
			max_stack : max_stack,
			max_locals : max_locals,
			code : code,
			exceptions : exceptions
		}
	}


	// ----------------------------------------------
	pub fn decode_opcodes()
	{
		// TODO
	}
}

