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

// based on http://docs.oracle.com/javase/specs/jvms/se7/html/jvms-4.html#jvms-4.4

extern mod std;
use std::num::FromPrimitive;
use std::io::Reader;

pub static ACC_PUBLIC : uint = 0x1;
pub static ACC_PRIVATE : uint = 0x2;
pub static ACC_PROTECTED : uint = 0x4;
pub static ACC_STATIC : uint = 0x8;
pub static ACC_FINAL : uint = 0x10;
pub static ACC_SYNCHRONIZED : uint = 0x20;
pub static ACC_VOLATILE : uint = 0x40;
pub static ACC_TRANSIENT: uint = 0x80;
pub static ACC_NATIVE : uint = 0x100;
pub static ACC_INTERFACE : uint = 0x200;
pub static ACC_ABSTRACT : uint = 0x400;
pub static ACC_STRICTFP : uint = 0x800;
pub static ACC_SYNTHETIC : uint = 0x1000;
pub static ACC_ANNOTATION : uint = 0x2000;
pub static ACC_ENUM : uint = 0x4000;


#[deriving(FromPrimitive)]
#[deriving(ToStr)]
pub enum ConstantPoolTags {
	CONSTANT_class = 7,
	CONSTANT_fieldref = 9,
	CONSTANT_methodref = 10,
	CONSTANT_ifacemethodref = 11,
	CONSTANT_string = 8,
	CONSTANT_integer = 3,
	CONSTANT_float = 4,
	CONSTANT_long = 5,
	CONSTANT_double = 6,
	CONSTANT_nameandtype = 12,
	CONSTANT_utf8 = 1,
	CONSTANT_methodhandle = 15,
	CONSTANT_methodtype = 16,
	CONSTANT_invokedynamic = 18
}


pub enum Constant {
	CONSTANT_class_info(u16),
	CONSTANT_fieldref_info(u16, u16),
	CONSTANT_methodref_info(u16, u16),
	CONSTANT_ifacemethodref_info(u16, u16),
	CONSTANT_string_info(u16),
	CONSTANT_integer_info(i32),
	CONSTANT_float_info(f32),
	CONSTANT_long_info(i64),
	CONSTANT_double_info(f64),
	CONSTANT_nameandtype_info(u16, u16),
	CONSTANT_utf8_info(~str),
	CONSTANT_methodhandle_info(u8, u16),
	CONSTANT_methodtype_info(u16),
	CONSTANT_invokedynamic_info(u16, u16)
}

