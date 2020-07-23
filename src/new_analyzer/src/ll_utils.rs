use inkwell::values::*;
use inkwell::basic_block::BasicBlock;
use inkwell::module::Module;
use either::Either;

pub struct FunctionIterator<'ctx> {
    next_function: Option<FunctionValue<'ctx>>
}

impl<'ctx> Iterator for FunctionIterator<'ctx> {
    type Item = FunctionValue<'ctx>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_function {
            Some(f) => {
                self.next_function = f.get_next_function();
                Some(f)
            }
            None => None
        }
    }
}

pub trait CreateFunctionIterator<'ctx> {
    fn iter_functions(&self) -> FunctionIterator<'ctx>;
}

impl<'ctx> CreateFunctionIterator<'ctx> for Module<'ctx> {
    fn iter_functions(&self) -> FunctionIterator<'ctx> {
        FunctionIterator {
            next_function: self.get_first_function()
        }
    }
}

pub struct InstructionIterator<'ctx> {
    next_instruction: Option<InstructionValue<'ctx>>
}

impl<'ctx> Iterator for InstructionIterator<'ctx> {
    type Item = InstructionValue<'ctx>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_instruction {
            Some(i) => {
                self.next_instruction = i.get_next_instruction();
                Some(i)
            }
            None => None
        }
    }
}

pub trait CreateInstructionIterator<'ctx> {
    fn iter_instructions(&self) -> InstructionIterator<'ctx>;
}

impl<'ctx> CreateInstructionIterator<'ctx> for BasicBlock<'ctx> {
    fn iter_instructions(&self) -> InstructionIterator<'ctx> {
        InstructionIterator { next_instruction: self.get_first_instruction() }
    }
}

pub fn callee_of_call_instr<'ctx>(module: &Module<'ctx>, i: InstructionValue<'ctx>) -> Option<FunctionValue<'ctx>> {
    if i.get_opcode() == InstructionOpcode::Call {
        let maybe_callee = i.get_operand(i.get_num_operands() - 1);
        match maybe_callee {
            Some(Either::Left(BasicValueEnum::PointerValue(pt))) => {
                let fname = pt.get_name();
                module.get_function(&fname.to_string_lossy())
            },
            _ => None
        }
    } else { None }
}