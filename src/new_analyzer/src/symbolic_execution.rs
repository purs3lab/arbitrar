use std::{io, io::{Write}};
use clap::{App, Arg, ArgMatches};
use llir::values::*;
use llir::types::*;
// use inkwell::{basic_block::BasicBlock, values::{InstructionValue, FunctionValue, PointerValue, InstructionOpcode, BasicValueEnum}};
// use llvm_sys::prelude::LLVMValueRef;
use std::rc::Rc;
// use petgraph::graph::{DiGraph, NodeIndex};
use rayon::prelude::*;
// use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::context::AnalyzerContext;
// use crate::ll_utils::*;
use crate::options::Options;
use crate::semantics::*;
use crate::slicer::Slice;

pub struct SymbolicExecutionOptions {
  pub max_trace_per_slice: usize,
  pub max_explored_trace_per_slice: usize,
  pub max_node_per_trace: usize,
  pub no_trace_reduction: bool,
}

impl Options for SymbolicExecutionOptions {
  fn setup_parser<'a>(app: App<'a>) -> App<'a> {
    app.args(&[
      Arg::new("max_trace_per_slice")
        .value_name("MAX_TRACE_PER_SLICE")
        .takes_value(true)
        .long("max-trace-per-slice")
        .about("The maximum number of generated trace per slice")
        .default_value("50"),
      Arg::new("max_explored_trace_per_slice")
        .value_name("MAX_EXPLORED_TRACE_PER_SLICE")
        .takes_value(true)
        .long("max-explored-trace-per-slice")
        .about("The maximum number of explroed trace per slice")
        .default_value("1000"),
      Arg::new("max_node_per_trace")
        .value_name("MAX_NODE_PER_TRACE")
        .takes_value(true)
        .long("max-node-per-trace")
        .default_value("1000"),
      Arg::new("no_reduce_trace")
        .long("no-reduce-trace")
        .about("No trace reduction"),
    ])
  }

  fn from_matches(matches: &ArgMatches) -> Result<Self, String> {
    Ok(Self {
      max_trace_per_slice: matches.value_of_t::<usize>("max_trace_per_slice").unwrap(),
      max_explored_trace_per_slice: matches.value_of_t::<usize>("max_explored_trace_per_slice").unwrap(),
      max_node_per_trace: matches.value_of_t::<usize>("max_node_per_trace").unwrap(),
      no_trace_reduction: matches.is_present("no-reduce-trace"),
    })
  }
}

#[derive(Debug)]
pub struct MetaData {
  pub proper_trace_count: usize,
  pub path_unsat_trace_count: usize,
  pub branch_explored_trace_count: usize,
  pub duplicate_trace_count: usize,
  pub no_target_trace_count: usize,
  pub exceeding_length_trace_count: usize,
  pub unreachable_trace_count: usize,
  pub explored_trace_count: usize,
}

impl MetaData {
  pub fn new() -> Self {
    MetaData {
      proper_trace_count: 0,
      path_unsat_trace_count: 0,
      branch_explored_trace_count: 0,
      duplicate_trace_count: 0,
      no_target_trace_count: 0,
      exceeding_length_trace_count: 0,
      unreachable_trace_count: 0,
      explored_trace_count: 0,
    }
  }

  pub fn combine(self, other: Self) -> Self {
    MetaData {
      proper_trace_count: self.proper_trace_count + other.proper_trace_count,
      path_unsat_trace_count: self.path_unsat_trace_count + other.path_unsat_trace_count,
      branch_explored_trace_count: self.branch_explored_trace_count + other.branch_explored_trace_count,
      duplicate_trace_count: self.duplicate_trace_count + other.duplicate_trace_count,
      no_target_trace_count: self.no_target_trace_count + other.no_target_trace_count,
      exceeding_length_trace_count: self.exceeding_length_trace_count + other.exceeding_length_trace_count,
      unreachable_trace_count: self.unreachable_trace_count + other.unreachable_trace_count,
      explored_trace_count: self.explored_trace_count + other.explored_trace_count,
    }
  }

  pub fn incr_proper(&mut self) {
    self.proper_trace_count += 1;
    self.explored_trace_count += 1;
  }

  pub fn incr_path_unsat(&mut self) {
    self.path_unsat_trace_count += 1;
    self.explored_trace_count += 1;
  }

  pub fn incr_branch_explored(&mut self) {
    self.branch_explored_trace_count += 1;
    self.explored_trace_count += 1;
  }

  pub fn incr_duplicated(&mut self) {
    self.duplicate_trace_count += 1;
    self.explored_trace_count += 1;
  }

  pub fn incr_no_target(&mut self) {
    self.no_target_trace_count += 1;
    self.explored_trace_count += 1;
  }

  pub fn incr_exceeding_length(&mut self) {
    self.exceeding_length_trace_count += 1;
    self.explored_trace_count += 1;
  }

  pub fn incr_unreachable(&mut self) {
    self.unreachable_trace_count += 1;
    self.explored_trace_count += 1;
  }
}

pub type LocalMemory<'ctx> = HashMap<Instruction<'ctx>, Rc<Value>>;

#[derive(Clone)]
pub struct StackFrame<'ctx> {
  pub function: Function<'ctx>,
  pub instr: Option<(usize, CallInstruction<'ctx>)>,
  pub memory: LocalMemory<'ctx>,
  pub arguments: Vec<Rc<Value>>,
}

impl<'ctx> StackFrame<'ctx> {
  pub fn entry(function: Function<'ctx>) -> Self {
    Self {
      function,
      instr: None,
      memory: LocalMemory::new(),
      arguments: (0..function.num_params())
        .map(|i| Rc::new(Value::Argument(i as usize)))
        .collect(),
    }
  }
}

pub type Stack<'ctx> = Vec<StackFrame<'ctx>>;

pub trait StackTrait<'ctx> {
  fn top(&self) -> &StackFrame<'ctx>;

  fn top_mut(&mut self) -> &mut StackFrame<'ctx>;
}

impl<'ctx> StackTrait<'ctx> for Stack<'ctx> {
  fn top(&self) -> &StackFrame<'ctx> {
    &self[self.len() - 1]
  }

  fn top_mut(&mut self) -> &mut StackFrame<'ctx> {
    let id = self.len() - 1;
    &mut self[id]
  }
}

pub type Memory = HashMap<Rc<Location>, Rc<Value>>;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchDirection<'ctx> {
  pub from: Block<'ctx>,
  pub to: Block<'ctx>,
}

pub type VisitedBranch<'ctx> = HashSet<BranchDirection<'ctx>>;

// pub type GlobalUsage<'ctx> = HashMap<GlobalValue<'ctx>, InstructionValue<'ctx>>;

// #[derive(Clone)]
// pub struct TraceNode<'ctx> {
//   pub instr: InstructionValue<'ctx>,
//   pub semantics: Instruction,
//   pub result: Option<Rc<Value>>,
// }

// impl<'ctx> std::fmt::Debug for TraceNode<'ctx> {
//   fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//     std::fmt::Debug::fmt(&self.semantics, f)
//   }
// }

#[derive(Clone)]
pub struct TraceNode {
  pub semantics: Semantics,
  pub result: Option<Rc<Value>>,
}

impl std::fmt::Debug for TraceNode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    std::fmt::Debug::fmt(&self.semantics, f)
  }
}

// #[derive(Clone)]
// pub enum TraceGraphEdge {
//   DefUse,
//   ControlFlow,
// }

// pub type TraceGraph<'ctx> = DiGraph<TraceNode<'ctx>, TraceGraphEdge>;

// pub trait TraceGraphTrait<'ctx> {
//   fn to_json(&self) -> Json;

//   fn reduce(self, target: NodeIndex) -> Self;
// }

// impl<'ctx> TraceGraphTrait<'ctx> for TraceGraph<'ctx> {
//   fn to_json(&self) -> Json {
//     Json::Null
//   }

//   fn reduce(self, target: NodeIndex) -> Self {
//     self
//   }
// }

pub type BlockTrace<'ctx> = Vec<Block<'ctx>>;

pub trait BlockTraceTrait<'ctx> {
  fn equals(&self, other: &Self) -> bool;
}

impl<'ctx> BlockTraceTrait<'ctx> for BlockTrace<'ctx> {
  fn equals(&self, other: &Self) -> bool {
    if self.len() == other.len() {
      for i in 0..self.len() {
        if self[i] != other[i] {
          return false;
        }
      }
      true
    } else {
      false
    }
  }
}

pub type Trace = Vec<TraceNode>;

pub trait TraceTrait {
  fn print(&self);
}

impl TraceTrait for Trace {
  fn print(&self) {
    for node in self.iter() {
      match &node.result {
        Some(result) => println!("{:?} -> {:?}", node.semantics, result),
        None => println!("{:?}", node.semantics),
      }
    }
  }
}

#[derive(Clone)]
pub enum FinishState {
  ProperlyReturned,
  BranchExplored,
  ExceedingMaxTraceLength,
  Unreachable,
}

#[derive(Debug, Clone)]
pub struct Constraint {
  pub cond: Comparison,
  pub branch: bool,
}

#[derive(Clone)]
pub struct State<'ctx> {
  pub stack: Stack<'ctx>,
  pub memory: Memory,
  pub visited_branch: VisitedBranch<'ctx>,
  // pub global_usage: GlobalUsage<'ctx>,
  pub block_trace: BlockTrace<'ctx>,
  pub trace: Trace,
  pub target_node: Option<usize>,
  pub prev_block: Option<Block<'ctx>>,
  pub finish_state: FinishState,
  pub pointer_value_id_map: HashMap<GenericValue<'ctx>, usize>,
  pub constraints: Vec<Constraint>,

  // Identifiers
  alloca_id: usize,
  symbol_id: usize,
  pointer_value_id: usize,
}

impl<'ctx> State<'ctx> {
  pub fn new(slice: &Slice<'ctx>) -> Self {
    Self {
      stack: vec![StackFrame::entry(slice.entry)],
      memory: Memory::new(),
      visited_branch: VisitedBranch::new(),
      // global_usage: GlobalUsage::new(),
      block_trace: BlockTrace::new(),
      trace: Vec::new(),
      target_node: None,
      prev_block: None,
      finish_state: FinishState::ProperlyReturned,
      pointer_value_id_map: HashMap::new(),
      constraints: Vec::new(),
      alloca_id: 0,
      symbol_id: 0,
      pointer_value_id: 0,
    }
  }

  pub fn new_alloca_id(&mut self) -> usize {
    let result = self.alloca_id;
    self.alloca_id += 1;
    result
  }

  pub fn new_symbol_id(&mut self) -> usize {
    let result = self.symbol_id;
    self.symbol_id += 1;
    result
  }

  pub fn new_pointer_value_id(&mut self, pv: GenericValue<'ctx>) -> usize {
    let result = self.pointer_value_id;
    self.pointer_value_id += 1;
    self.pointer_value_id_map.insert(pv, result);
    result
  }

  pub fn add_constraint(&mut self, cond: Comparison, branch: bool) {
    self.constraints.push(Constraint { cond, branch });
  }

  pub fn path_satisfactory(&self) -> bool {
    use z3::*;
    let z3_ctx = Context::new(&z3::Config::default());
    let solver = Solver::new(&z3_ctx);
    let mut symbol_map = HashMap::new();
    let mut symbol_id = 0;
    for Constraint { cond, branch } in self.constraints.iter() {
      match cond.into_z3_ast(&mut symbol_map, &mut symbol_id, &z3_ctx) {
        Some(cond) => {
          let formula = if *branch { cond } else { cond.not() };
          solver.assert(&formula);
        }
        _ => (),
      }
    }
    match solver.check() {
      SatResult::Sat | SatResult::Unknown => true,
      _ => false,
    }
  }

  pub fn dump_json(&self, _path: PathBuf) {
    // TODO
  }
}

pub struct Work<'ctx> {
  pub block: Block<'ctx>,
  pub state: State<'ctx>,
}

impl<'ctx> Work<'ctx> {
  pub fn entry(slice: &Slice<'ctx>) -> Self {
    let block = slice.entry.first_block().unwrap();
    let state = State::new(slice);
    Self { block, state }
  }
}

pub struct Environment<'ctx> {
  pub slice: Slice<'ctx>,
  pub work_list: Vec<Work<'ctx>>,
  pub block_traces: Vec<BlockTrace<'ctx>>,
  pub call_id: usize,
}

impl<'ctx> Environment<'ctx> {
  pub fn new(slice: Slice<'ctx>) -> Self {
    let initial_work = Work::entry(&slice);
    Self {
      slice,
      work_list: vec![initial_work],
      block_traces: vec![],
      call_id: 0,
    }
  }

  pub fn has_work(&self) -> bool {
    !self.work_list.is_empty()
  }

  pub fn pop_work(&mut self) -> Work<'ctx> {
    self.work_list.pop().unwrap()
  }

  pub fn add_work(&mut self, work: Work<'ctx>) {
    self.work_list.push(work);
  }

  pub fn new_call_id(&mut self) -> usize {
    let result = self.call_id;
    self.call_id += 1;
    result
  }

  pub fn has_duplicate(&self, block_trace: &BlockTrace<'ctx>) -> bool {
    for other_block_trace in self.block_traces.iter() {
      if block_trace.equals(other_block_trace) {
        return true;
      }
    }
    false
  }
}

pub struct SymbolicExecutionContext<'a, 'ctx> {
  pub ctx: &'a AnalyzerContext<'ctx>,
  pub options: SymbolicExecutionOptions,
}

unsafe impl<'a, 'ctx> Sync for SymbolicExecutionContext<'a, 'ctx> {}

impl<'a, 'ctx> SymbolicExecutionContext<'a, 'ctx> {
  pub fn new(ctx: &'a AnalyzerContext<'ctx>) -> Result<Self, String> {
    let options = SymbolicExecutionOptions::from_matches(&ctx.args)?;
    Ok(Self { ctx, options })
  }

  pub fn trace_file_name(&self, func_name: String, slice_id: usize, trace_id: usize) -> PathBuf {
    Path::new(self.ctx.options.output_path.as_str())
      .join("traces")
      .join(func_name.as_str())
      .join(slice_id.to_string())
      .join(trace_id.to_string())
  }

  pub fn execute_function(
    &self,
    instr_node_id: usize,
    instr: CallInstruction<'ctx>,
    func: Function<'ctx>,
    args: Vec<Rc<Value>>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    match func.first_block() {
      Some(block) => {
        let stack_frame = StackFrame {
          function: func,
          instr: Some((instr_node_id, instr)),
          memory: LocalMemory::new(),
          arguments: args,
        };
        state.stack.push(stack_frame);
        self.execute_block(block, state, env);
      }
      None => {}
    }
  }

  pub fn execute_block(&self, block: Block<'ctx>, state: &mut State<'ctx>, env: &mut Environment<'ctx>) {
    state.block_trace.push(block);
    self.execute_instr(block.first_instruction(), state, env)
  }

  pub fn execute_instr(
    &self,
    instr: Option<Instruction<'ctx>>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    if state.trace.len() > self.options.max_node_per_trace {
      state.finish_state = FinishState::ExceedingMaxTraceLength;
      return;
    }

    match instr {
      Some(instr) => {
        use Instruction::*;
        match instr {
          Return(ret) => self.transfer_ret_instr(ret, state, env),
          Branch(br) => self.transfer_br_instr(br, state, env),
          Switch(swi) => self.transfer_switch_instr(swi, state, env),
          Call(call) => self.transfer_call_instr(call, state, env),
          Alloca(alloca) => self.transfer_alloca_instr(alloca, state, env),
          Store(st) => self.transfer_store_instr(st, state, env),
          ICmp(icmp) => self.transfer_icmp_instr(icmp, state, env),
          Load(ld) => self.transfer_load_instr(ld, state, env),
          Phi(phi) => self.transfer_phi_instr(phi, state, env),
          GetElementPtr(gep) => self.transfer_gep_instr(gep, state, env),
          Unreachable(unr) => self.transfer_unreachable_instr(unr, state, env),
          Binary(bin) => self.transfer_binary_instr(bin, state, env),
          Unary(una) => self.transfer_unary_instr(una, state, env),
          _ => self.transfer_instr(instr, state, env),
        };
      }
      None => {
        state.finish_state = FinishState::ProperlyReturned;
      }
    }
  }

  pub fn eval_operand_value(&self, _state: &mut State<'ctx>, _operand: Operand<'ctx>) -> Rc<Value> {
    // TODO
    Rc::new(Value::Unknown)
  }

  pub fn eval_operand_location(&self, _state: &mut State<'ctx>, _operand: Operand<'ctx>) -> Rc<Location> {
    // TODO
    Rc::new(Location::Unknown)
  }

  pub fn load_from_memory(&self, state: &mut State<'ctx>, location: Rc<Location>) -> Rc<Value> {
    match &*location {
      Location::Unknown => Rc::new(Value::Unknown),
      _ => match state.memory.get(&location) {
        Some(value) => value.clone(),
        None => {
          let symbol_id = state.new_symbol_id();
          let value = Rc::new(Value::Symbol(symbol_id));
          state.memory.insert(location, value.clone());
          value
        }
      },
    }
  }

  pub fn transfer_ret_instr(
    &self,
    instr: ReturnInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    // First evaluate the return operand. There might not be one
    let val = instr.op().map(|val| self.eval_operand_value(state, val));
    state.trace.push(TraceNode {
      // instr,
      semantics: Semantics::Return { op: val.clone() },
      result: None,
    });

    // Then we peek the stack frame
    let stack_frame = state.stack.pop().unwrap(); // There has to be a stack on the top
    match stack_frame.instr {
      Some((node_id, call_site)) => {
        let call_site_frame = state.stack.top_mut(); // If call site exists then there must be a stack top
        if let Some(op0) = val {
          state.trace[node_id].result = Some(op0.clone());
          call_site_frame.memory.insert(call_site.as_instruction(), op0);
        }
        self.execute_instr(call_site.next_instruction(), state, env);
      }

      // If no call site then we are in the entry function. We will end the execution
      None => {
        state.finish_state = FinishState::ProperlyReturned;
      }
    }
  }

  pub fn transfer_br_instr(&self, instr: BranchInstruction<'ctx>, state: &mut State<'ctx>, env: &mut Environment<'ctx>) {
    let curr_blk = instr.parent_block(); // We assume instruction always has parent block
    state.prev_block = Some(curr_blk);
    match instr {
      // We assume instr is branch instruction
      BranchInstruction::Conditional(cb) => {
        let cond = self.eval_operand_value(state, cb.condition().into());
        let comparison = cond.as_comparison();
        // TODO
        // let is_loop_blk = curr_blk.is_loop_block(&self.ctx.llcontext());
        let is_loop_blk = false;
        let then_br = BranchDirection {
          from: curr_blk,
          to: cb.then_block(),
        };
        let else_br = BranchDirection {
          from: curr_blk,
          to: cb.else_block(),
        };
        let visited_then = state.visited_branch.contains(&then_br);
        let visited_else = state.visited_branch.contains(&else_br);
        if !visited_then {
          // Check if we need to add a work for else branch
          if !visited_else {
            // First add else branch into work
            let mut else_state = state.clone();
            if let Some(comparison) = comparison.clone() {
              if !is_loop_blk {
                else_state.add_constraint(comparison, false);
              }
            }
            else_state.visited_branch.insert(else_br);
            else_state.trace.push(TraceNode {
              // instr,
              result: None,
              semantics: Semantics::ConditionalBr {
                cond: cond.clone(),
                br: Branch::Else,
                begin_loop: false,
              },
            });
            let else_work = Work {
              block: cb.else_block(),
              state: else_state,
            };
            env.add_work(else_work);
          }

          // Then execute the then branch
          if let Some(comparison) = comparison {
            if !is_loop_blk {
              state.add_constraint(comparison, true);
            }
          }
          state.visited_branch.insert(then_br);
          state.trace.push(TraceNode {
            // instr: instr,
            result: None,
            semantics: Semantics::ConditionalBr { cond, br: Branch::Then, begin_loop: is_loop_blk },
          });
          self.execute_block(cb.then_block(), state, env);
        } else if !visited_else {
          // Execute the else branch
          if let Some(comparison) = comparison {
            if !is_loop_blk {
              state.add_constraint(comparison.clone(), false);
            }
          }
          state.visited_branch.insert(else_br);
          state.trace.push(TraceNode {
            // instr: instr,
            semantics: Semantics::ConditionalBr { cond, br: Branch::Else, begin_loop: false },
            result: None,
          });
          self.execute_block(cb.else_block(), state, env);
        } else {
          // If both then and else are visited, stop the execution with BranchExplored
          state.finish_state = FinishState::BranchExplored;
        }
      }
      BranchInstruction::Unconditional(ub) => {
        state.trace.push(TraceNode {
          // instr: instr,
          semantics: Semantics::UnconditionalBr {
            end_loop: false, // TODO: instr.is_loop(&self.ctx.llmod.get_context()),
          },
          result: None,
        });
        self.execute_block(ub.target_block(), state, env);
      }
    }
  }

  pub fn transfer_switch_instr(
    &self,
    instr: SwitchInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let curr_blk = instr.parent_block();
    state.prev_block = Some(curr_blk);
    let cond = self.eval_operand_value(state, instr.condition().into());
    let default_br = BranchDirection {
      from: curr_blk,
      to: instr.default_block(),
    };
    let branches = instr
      .branches()
      .iter()
      .map(|(_, to)| BranchDirection {
        from: curr_blk,
        to: *to,
      })
      .collect::<Vec<_>>();
    let node = TraceNode {
      // instr,
      semantics: Semantics::Switch { cond },
      result: None,
    };
    state.trace.push(node);

    // Insert branches as work if not visited
    for bd in branches {
      if !state.visited_branch.contains(&bd) {
        let mut br_state = state.clone();
        br_state.visited_branch.insert(bd);
        let br_work = Work {
          block: bd.to,
          state: br_state,
        };
        env.add_work(br_work);
      }
    }

    // Execute default branch
    if !state.visited_branch.contains(&default_br) {
      state.visited_branch.insert(default_br);
      self.execute_block(instr.default_block(), state, env);
    }
  }

  pub fn transfer_call_instr(
    &self,
    instr: CallInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let callee_name = instr.callee().value().name();
    // If no name or llvm related
    if callee_name.is_none() || callee_name.clone().unwrap().contains("llvm.") {
      self.execute_instr(instr.next_instruction(), state, env);
    } else {
      let callee_name = callee_name.unwrap();
      let args: Vec<Rc<Value>> = instr
        .arguments()
        .into_iter()
        .map(|v| self.eval_operand_value(state, v))
        .collect();

      // Store call node id
      let node_id = state.trace.len();

      // Add the node into the trace
      let semantics = Semantics::Call {
        func: callee_name.clone(),
        args: args.clone(),
      };
      let node = TraceNode {
        // instr,
        semantics,
        result: None,
      };
      state.trace.push(node);

      // Check if this is the target function call
      if instr.as_instruction() == env.slice.instr && state.target_node.is_none() {
        state.target_node = Some(node_id);
      }

      // Check if we need to go into the function
      match instr.callee_function() {
        Some(callee) if !callee.is_declaration_only() && env.slice.functions.contains(&callee) => {
          self.execute_function(node_id, instr, callee, args, state, env);
        }
        _ => {
          let call_id = env.new_call_id();
          let result = Rc::new(Value::Call {
            id: call_id,
            func: callee_name,
            args,
          });
          state.stack.top_mut().memory.insert(instr.as_instruction(), result);
          self.execute_instr(instr.next_instruction(), state, env);
        }
      }
    }
  }

  pub fn transfer_alloca_instr(
    &self,
    instr: AllocaInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    // Lazy evaluate alloca instructions
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_store_instr(
    &self,
    instr: StoreInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let loc = self.eval_operand_location(state, instr.location());
    let val = self.eval_operand_value(state, instr.value());
    state.memory.insert(loc.clone(), val.clone());
    let node = TraceNode {
      // instr: instr,
      semantics: Semantics::Store { loc, val },
      result: None,
    };
    state.trace.push(node);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_load_instr(
    &self,
    instr: LoadInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let loc = self.eval_operand_location(state, instr.location());
    let res = self.load_from_memory(state, loc.clone());
    let node = TraceNode {
      // instr: instr,
      semantics: Semantics::Load { loc },
      result: Some(res.clone()),
    };
    state.trace.push(node);
    state.stack.top_mut().memory.insert(instr.as_instruction(), res);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_icmp_instr(
    &self,
    instr: ICmpInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let pred = instr.predicate(); // ICMP must have a predicate
    let op0 = self.eval_operand_value(state, instr.op0());
    let op1 = self.eval_operand_value(state, instr.op1());
    let res = Rc::new(Value::Comparison {
      pred,
      op0: op0.clone(),
      op1: op1.clone(),
    });
    let semantics = Semantics::Compare { pred, op0, op1 };
    let node = TraceNode {
      // instr,
      semantics,
      result: Some(res.clone()),
    };
    state.trace.push(node);
    state.stack.top_mut().memory.insert(instr.as_instruction(), res);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_phi_instr(
    &self,
    instr: PhiInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let prev_blk = state.prev_block.unwrap();
    let incoming_val = instr.incomings().iter().find(|incoming| incoming.block == prev_blk).unwrap().value;
    let res = self.eval_operand_value(state, incoming_val);
    state.stack.top_mut().memory.insert(instr.as_instruction(), res);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_gep_instr(
    &self,
    instr: GetElementPtrInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let loc = self.eval_operand_location(state, instr.location());
    let indices = instr
      .indices()
      .iter()
      .map(|index| self.eval_operand_value(state, *index))
      .collect::<Vec<_>>();
    let res = Rc::new(Value::Location(Rc::new(Location::GetElementPtr(
      loc.clone(),
      indices.clone(),
    ))));
    let node = TraceNode {
      // instr,
      semantics: Semantics::GetElementPtr {
        loc: loc.clone(),
        indices,
      },
      result: Some(res.clone()),
    };
    state.trace.push(node);
    state.stack.top_mut().memory.insert(instr.as_instruction(), res);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_binary_instr(
    &self,
    instr: BinaryInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let op = instr.opcode();
    let v0 = self.eval_operand_value(state, instr.op0());
    let v1 = self.eval_operand_value(state, instr.op1());
    let res = Rc::new(Value::BinaryOperation {
      op,
      op0: v0.clone(),
      op1: v1.clone(),
    });
    let node = TraceNode {
      // instr,
      semantics: Semantics::BinaryOperation { op, op0: v0, op1: v1 },
      result: Some(res.clone()),
    };
    state.trace.push(node);
    state.stack.top_mut().memory.insert(instr.as_instruction(), res);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_unary_instr(
    &self,
    instr: UnaryInstruction<'ctx>,
    state: &mut State<'ctx>,
    env: &mut Environment<'ctx>,
  ) {
    let op = instr.opcode();
    let op0 = self.eval_operand_value(state, instr.op0());
    let node = TraceNode {
      // instr,
      semantics: Semantics::UnaryOperation { op, op0: op0.clone() },
      result: Some(op0.clone()),
    };
    state.trace.push(node);
    state.stack.top_mut().memory.insert(instr.as_instruction(), op0);
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn transfer_unreachable_instr(
    &self,
    _: UnreachableInstruction<'ctx>,
    state: &mut State<'ctx>,
    _: &mut Environment<'ctx>,
  ) {
    state.finish_state = FinishState::Unreachable;
  }

  pub fn transfer_instr(&self, instr: Instruction<'ctx>, state: &mut State<'ctx>, env: &mut Environment<'ctx>) {
    self.execute_instr(instr.next_instruction(), state, env);
  }

  pub fn continue_execution(&self, metadata: &MetaData) -> bool {
    metadata.explored_trace_count < self.options.max_explored_trace_per_slice
      && metadata.proper_trace_count < self.options.max_trace_per_slice
  }

  pub fn execute_slice(&self, slice: Slice<'ctx>, slice_id: usize) -> MetaData {
    let mut metadata = MetaData::new();
    let mut env = Environment::new(slice);
    while env.has_work() && self.continue_execution(&metadata) {
      if cfg!(debug_assertions) {
        println!("=========== {} ==========", metadata.explored_trace_count);
      }

      let mut work = env.pop_work();
      self.execute_block(work.block, &mut work.state, &mut env);
      match work.state.target_node {
        Some(_target_id) => match work.state.finish_state {
          FinishState::ProperlyReturned => {
            // if !self.options.no_trace_reduction {
            //   work.state.trace_graph = work.state.trace_graph.reduce(target_id);
            // }
            if !env.has_duplicate(&work.state.block_trace) {
              if work.state.path_satisfactory() {
                let trace_id = metadata.proper_trace_count;
                let path = self.trace_file_name(env.slice.target_function_name(), slice_id, trace_id);
                if cfg!(debug_assertions) {
                  work.state.trace.print();
                }
                work.state.dump_json(path);
                metadata.incr_proper();
              } else {
                if cfg!(debug_assertions) {
                  for cons in work.state.constraints {
                    println!("{:?}", cons);
                  }
                  println!("Path unsat");
                }
                metadata.incr_path_unsat()
              }
            } else {
              if cfg!(debug_assertions) {
                println!("Duplicated");
              }
              metadata.incr_duplicated()
            }
          }
          FinishState::BranchExplored => {
            if cfg!(debug_assertions) {
              println!("Branch explored");
            }
            metadata.incr_branch_explored()
          }
          FinishState::ExceedingMaxTraceLength => {
            if cfg!(debug_assertions) {
              println!("Exceeding Length");
            }
            metadata.incr_exceeding_length()
          }
          FinishState::Unreachable => {
            if cfg!(debug_assertions) {
              println!("Unreachable");
            }
            metadata.incr_unreachable()
          }
        },
        None => metadata.incr_no_target(),
      }
    }

    print!("Executing Slice {}\r", match slice_id % 3 {
      0 => '|', 1 => '/', 2 => '-', _ => '\\'
    });
    io::stdout().flush().unwrap();

    metadata
  }

  pub fn execute_slices(&self, slices: Vec<Slice<'ctx>>) -> MetaData {
    let f = |meta: MetaData, (slice_id, slice): (usize, Slice<'ctx>)| meta.combine(self.execute_slice(slice, slice_id));
    if self.ctx.options.use_serial {
      slices.into_iter().enumerate().fold(MetaData::new(), f)
    } else {
      slices
        .into_par_iter()
        .enumerate()
        .fold(|| MetaData::new(), f)
        .reduce(|| MetaData::new(), MetaData::combine)
    }
  }
}
