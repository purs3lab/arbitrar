use clap::{App, Arg, ArgMatches};
use inkwell::values::*;
use petgraph::{graph::EdgeIndex, Direction};
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashSet;
use std::slice::Chunks;

use crate::call_graph::CallGraph;
use crate::context::AnalyzerContext;
use crate::ll_utils::*;
use crate::options::Options;

pub struct Slice<'ctx> {
  pub entry: FunctionValue<'ctx>,
  pub caller: FunctionValue<'ctx>,
  pub callee: FunctionValue<'ctx>,
  pub instr: InstructionValue<'ctx>,
  pub functions: HashSet<FunctionValue<'ctx>>,
}

unsafe impl<'ctx> Send for Slice<'ctx> {}

pub struct SlicerOptions {
  pub depth: u8,
  pub target_inclusion_filter: Option<String>,
  pub target_exclusion_filter: Option<String>,
  pub entry_filter: Option<String>,
  pub reduce_slice: bool,
  pub use_batch: bool,
  pub batch_size: u32,
}

impl Options for SlicerOptions {
  fn setup_parser<'a>(app: App<'a>) -> App<'a> {
    app.args(&[
      Arg::new("depth")
        .value_name("DEPTH")
        .takes_value(true)
        .short('d')
        .long("depth")
        .about("Slice depth")
        .default_value("1"),
      Arg::new("target_inclusion_filter")
        .value_name("INCLUDE_TARGET")
        .takes_value(true)
        .long("include-target")
        .about("Include target functions. In the form of Regex"),
      Arg::new("target_exclusion_filter")
        .value_name("EXCLUDE_TARGET")
        .takes_value(true)
        .long("exclude-target")
        .about("Exclude target functions. In the form of Regex"),
      Arg::new("entry_filter")
        .value_name("ENTRY_LOCATION")
        .takes_value(true)
        .long("entry-location")
        .about("Entry location filters. In the form of Regex"),
      Arg::new("reduce_slice")
        .long("reduce-slice")
        .about("Reduce slice using relevancy test"),
      Arg::new("use_batch").long("use-batch").about("Use batched execution"),
      Arg::new("batch_size")
        .value_name("BATCH_SIZE")
        .takes_value(true)
        .default_value("100")
        .long("batch-size"),
    ])
  }

  fn from_matches(matches: &ArgMatches) -> Result<Self, String> {
    Ok(Self {
      depth: matches
        .value_of_t::<u8>("depth")
        .map_err(|_| String::from("Cannot parse depth"))?,
      target_inclusion_filter: matches.value_of("target_inclusion_filter").map(String::from),
      target_exclusion_filter: matches.value_of("target_exclusion_filter").map(String::from),
      entry_filter: matches.value_of("entry_filter").map(String::from),
      reduce_slice: matches.is_present("reduce_slice"),
      use_batch: matches.is_present("use_batch"),
      batch_size: matches
        .value_of_t::<u32>("batch_size")
        .map_err(|_| String::from("Cannot parse batch size"))?,
    })
  }
}

pub struct SlicerContext<'a, 'ctx> {
  pub ctx: &'a AnalyzerContext<'ctx>,
  pub call_graph: &'a CallGraph<'ctx>,
  pub options: SlicerOptions,
}

unsafe impl<'a, 'ctx> Sync for SlicerContext<'a, 'ctx> {}

impl<'a, 'ctx> SlicerContext<'a, 'ctx> {
  pub fn new(ctx: &'a AnalyzerContext<'ctx>, call_graph: &'a CallGraph<'ctx>) -> Result<Self, String> {
    let options = SlicerOptions::from_matches(&ctx.args)?;
    Ok(SlicerContext {
      ctx,
      call_graph,
      options,
    })
  }

  pub fn relavant_edges(&self) -> Result<Vec<EdgeIndex>, String> {
    let inclusion_filter = match &self.options.target_inclusion_filter {
      Some(filter) => {
        let inclusion_regex =
          Regex::new(filter.as_str()).map_err(|_| String::from("Cannot parse target inclusion filter regex"))?;
        Some(inclusion_regex)
      }
      None => None,
    };
    let exclusion_filter = match &self.options.target_exclusion_filter {
      Some(filter) => {
        let exclusion_regex =
          Regex::new(filter.as_str()).map_err(|_| String::from("Cannot parse target exclusion filter regex"))?;
        Some(exclusion_regex)
      }
      None => None,
    };
    let mut edges = vec![];
    for callee_id in self.call_graph.node_indices() {
      let func = self.call_graph[callee_id];
      let func_name = func.function_name();
      let include_from_inclusion = match &inclusion_filter {
        Some(inclusion_regex) => {
          if inclusion_regex.is_match(func_name.as_str()) {
            None
          } else {
            Some(false)
          }
        }
        None => None,
      };
      let include = match include_from_inclusion {
        Some(i) => i,
        None => match &exclusion_filter {
          Some(exclusion_regex) => !exclusion_regex.is_match(func_name.as_str()),
          None => true,
        },
      };
      if include {
        for caller_id in self.call_graph.neighbors_directed(callee_id, Direction::Incoming) {
          edges.push(self.call_graph.find_edge(caller_id, callee_id).unwrap());
        }
      }
    }
    Ok(edges)
  }

  pub fn num_batches<'b>(&self, edges: &'b Vec<EdgeIndex>) -> u32 {
    if self.options.use_batch {
      (edges.len() as f32 / self.options.batch_size as f32).ceil() as u32
    } else {
      1
    }
  }

  pub fn batches<'b>(&self, edges: &'b Vec<EdgeIndex>) -> Chunks<'b, EdgeIndex> {
    if self.options.use_batch {
      edges.chunks(self.options.batch_size as usize)
    } else {
      edges.chunks(edges.len())
    }
  }

  pub fn slices_of_call_edge(&self, _edge_id: &EdgeIndex) -> Vec<Slice<'ctx>> {
    vec![]
  }

  pub fn slices_of_call_edges(&self, edges: &[EdgeIndex]) -> Vec<Slice<'ctx>> {
    edges
      .par_iter()
      .map(|edge_id: &EdgeIndex| -> Vec<Slice<'ctx>> { self.slices_of_call_edge(edge_id) })
      .flatten()
      .collect()
  }
}
