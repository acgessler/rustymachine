extern mod std;
extern mod extra;

use extra::arc::{MutexArc};

use classloader::*;
use class::{JavaClassFutureRef};
use util::{assert_is_err, assert_no_err};


// FieldDescriptor is modelled after the official grammar for Java field descriptors from 
// http://docs.oracle.com/javase/specs/jvms/se7/html/jvms-4.html#jvms-4.3.2


pub enum FieldDescriptor {
	// primitive data type
	FD_BaseType(BaseType),

	// object data type - the class reference contained
	// therein is not resolved to a class yet as doing so
	// immediately poses the danger of cyclic references,
	// and holding it in a Future makes the whole data
	// structure non-freezable.
	FD_ObjectType(~str),

	// adds one array dimension to the field's type
	FD_ArrayType(~FieldDescriptor)
}


#[deriving(FromPrimitive)]
#[deriving(ToStr)]
#[deriving(Eq)]
pub enum BaseType {
	BT_B_byte,     // = 'B',
	BT_C_char,     // = 'C',
	BT_D_double,   // = 'D',
	BT_F_float,    // = 'F',
	BT_I_int,      // = 'I',
	BT_J_long,     // = 'J',
	BT_S_short,    // = 'S',
	BT_Z_boolean,  // = 'Z'
}



pub struct JavaField {
	priv name : ~str,
	priv jtype : FieldDescriptor
	//
	//priv constant_value : ~str,
}


impl JavaField {

	// ----------------------------------------------
	pub fn new_from_string( name : &str, field_desc : &str) -> 
		Result<JavaField, ~str>
	{
		match JavaField::resolve_field_desc(field_desc) {
			Ok(t) => Ok(JavaField {
				name : name.into_owned(),
				jtype : t
			}),
			Err(s) => Err(s)
		}
	}


	// ----------------------------------------------
	pub fn resolve_field_desc(field_desc : &str) -> 
		Result<FieldDescriptor, ~str>
	{
		if field_desc.len() == 0 {
			return Err(~"empty field descriptor");
		}
		let head = field_desc[0] as char;
		let rest = field_desc.slice(1, field_desc.len());
		match head {
			// object types
			'L' => {
				if rest.len() != 0 && (rest[rest.len()-1] as char) == ';' {
					Ok(FD_ObjectType(rest.slice(0, rest.len() - 1).replace("/",".")))
				}
				else {
					Err(~"class name must end with ;")
				}
			},
			// array types
			'[' => {
				match JavaField::resolve_field_desc(rest) {
					Ok(fd) => Ok(FD_ArrayType(~fd)),
					Err(s) => Err(s)
				}
				
			},
			// primitive types
			'B'|'C'|'D'|'F'|'I'|'J'|'S'|'Z' => {
				if rest.len() == 0 {
					Ok(match head {
						'B' => FD_BaseType(BT_B_byte),
						'C' => FD_BaseType(BT_C_char),
						'D' => FD_BaseType(BT_D_double),
						'F' => FD_BaseType(BT_F_float),
						'I' => FD_BaseType(BT_I_int),
						'J' => FD_BaseType(BT_J_long),
						'S' => FD_BaseType(BT_S_short),
						'Z' => FD_BaseType(BT_Z_boolean),
						_ => fail!("invariant"),
					})
				}
				else {
					Err(format!("non-consumed trailing chars: {}", rest))
				}
			},
			_ => Err(format!("cannot parse, unrecognized character {}", head))
		}
	}
}







#[test]
fn test_field_desc_parsing() {
	// TODO: find better way to formulare these tests
	let mut cl = JavaField::resolve_field_desc(&"Ljava/lang/Object;");
	assert_no_err(&cl);
	match(cl) {
		Ok(FD_ObjectType(c)) => {
			assert!(c == ~"java.lang.Object")
		},
		_ => assert!(false)
	}

	cl = JavaField::resolve_field_desc(&"[[LEmptyClass;");
	assert_no_err(&cl);
	match cl {
		Ok(FD_ArrayType(~FD_ArrayType(~FD_ObjectType(c)))) => {
			assert!(c == ~"EmptyClass")
		},
		_ => assert!(false)
	}

	cl = JavaField::resolve_field_desc(&"B");
	assert_no_err(&cl);
	match cl {
		Ok(FD_BaseType(bt)) => assert!(bt == BT_B_byte),
		_ => assert!(false)
	}
}


#[test]
fn test_field_desc_parsing_fail() {
	assert_is_err(&JavaField::resolve_field_desc(&"Ljava/lang/Object"));
	assert_is_err(&JavaField::resolve_field_desc(&"Ljava/lang/Object;["));
	assert_is_err(&JavaField::resolve_field_desc(&""));
	assert_is_err(&JavaField::resolve_field_desc(&"b"));
	assert_is_err(&JavaField::resolve_field_desc(&"["));
}

