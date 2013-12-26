

// based on http://docs.oracle.com/javase/specs/jvms/se7/html/jvms-4.html#jvms-4.4

extern mod std;
use std::num::FromPrimitive;
use std::io::Reader;

static ACC_PUBLIC : uint = 0x1;
static ACC_PRIVATE : uint = 0x2;
static ACC_PROTECTED : uint = 0x4;
static ACC_STATIC : uint = 0x8;
static ACC_FINAL : uint = 0x10;
static ACC_SYNCHRONIZED : uint = 0x20;
static ACC_VOLATILE : uint = 0x40;
static ACC_TRANSIENT: uint = 0x80;
static ACC_NATIVE : uint = 0x100;
static ACC_ABSTRACT : uint = 0x400;
static ACC_STRICTFP : uint = 0x800;
static ACC_SYNTHETIC : uint = 0x1000;
static ACC_ANNOTATION : uint = 0x2000;
static ACC_ENUM : uint = 0x4000;


#[deriving(FromPrimitive)]
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

