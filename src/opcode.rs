

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



