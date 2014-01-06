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

// Maps from the instruction code found in the .class files
// to symbolic Opcode identifiers. This mapping is auto-generated using
// the compiler-generated FromPrimitive()-traits 

#[deriving(FromPrimitive)]
#[deriving(ToStr)]
pub enum Opcode {
	
	OpCode_nop = 0,
}


// Enum variant containing the decoded and linked forms of the opcodes.
// In this format, all extra bytes are attached to the opcode itself
// in the most natural representation possible. References to fields,
// classes, methods or any other Java symbol are resolved at this stage
// to alleviate further error checking during execution.
pub enum DecodedOpcode {

	DecodedOpcode_nop = 0,

}



