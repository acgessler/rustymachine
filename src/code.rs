

use classloader::{JavaClassRef};


pub struct ExceptionHandler
{
	start_pc : uint,
	end_pc : uint,
	handler_pc : uint,
	catch_type : JavaClassRef,
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

}

