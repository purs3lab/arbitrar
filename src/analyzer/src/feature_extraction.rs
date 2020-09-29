use llir::{types::*, Module};
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::feature_extractors::*;
use crate::options::*;
use crate::semantics::boxed::*;
use crate::utils::*;

#[derive(Deserialize)]
pub struct Slice {
  pub instr: String,
  pub entry: String,
  pub caller: String,
  pub callee: String,
  pub functions: Vec<String>,
}

impl Slice {}

#[derive(Deserialize)]
pub struct Instr {
  pub loc: String,
  pub sem: Semantics,
  pub res: Option<Value>,
}

#[derive(Deserialize)]
pub struct Trace {
  pub target: usize,
  pub instrs: Vec<Instr>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TraceIterDirection {
  Forward,
  Backward,
}

impl TraceIterDirection {
  pub fn is_forward(&self) -> bool {
    self == &Self::Forward
  }
}

impl Trace {
  pub fn target_result(&self) -> &Option<Value> {
    &self.target_instr().res
  }

  pub fn target_instr(&self) -> &Instr {
    &self.instrs[self.target]
  }

  pub fn target_args(&self) -> Vec<&Value> {
    self.target_instr().sem.call_args()
  }

  pub fn target_arg(&self, index: usize) -> &Value {
    self.target_instr().sem.call_args()[index]
  }

  pub fn iter_instrs(&self, dir: TraceIterDirection) -> Vec<&Instr> {
    if dir.is_forward() {
      self.instrs.iter().collect()
    } else {
      self.instrs.iter().rev().collect()
    }
  }

  pub fn iter_instrs_from_target(&self, dir: TraceIterDirection) -> Vec<&Instr> {
    self.iter_instrs_from(dir, self.target)
  }

  pub fn iter_instrs_from(&self, dir: TraceIterDirection, from: usize) -> Vec<&Instr> {
    if dir.is_forward() {
      self.instrs.iter().skip(from + 1).collect::<Vec<_>>()
    } else {
      self.instrs.iter().take(from).rev().collect::<Vec<_>>()
    }
  }
}

pub trait FeatureExtractorOptions: IOOptions + Send + Sync {
  fn causality_dictionary_size(&self) -> usize;
}

pub trait FeatureExtractor: Send + Sync {
  fn name(&self) -> String;

  fn filter<'ctx>(&self, target: &String, target_type: FunctionType<'ctx>) -> bool;

  fn init(&mut self, slice: &Slice, num_traces: usize, trace: &Trace);

  fn finalize(&mut self);

  fn extract(&self, slice: &Slice, trace: &Trace) -> serde_json::Value;
}

pub struct FeatureExtractors {
  extractors: Vec<Box<dyn FeatureExtractor>>,
}

impl FeatureExtractors {
  pub fn all(options: &impl FeatureExtractorOptions) -> Self {
    Self {
      extractors: vec![
        Box::new(ReturnValueFeatureExtractor::new()),
        Box::new(ReturnValueCheckFeatureExtractor::new()),
        Box::new(ArgumentPreconditionFeatureExtractor::new(0)),
        Box::new(ArgumentPreconditionFeatureExtractor::new(1)),
        Box::new(ArgumentPreconditionFeatureExtractor::new(2)),
        Box::new(ArgumentPreconditionFeatureExtractor::new(3)),
        Box::new(ArgumentPostconditionFeatureExtractor::new(0)),
        Box::new(ArgumentPostconditionFeatureExtractor::new(1)),
        Box::new(ArgumentPostconditionFeatureExtractor::new(2)),
        Box::new(ArgumentPostconditionFeatureExtractor::new(3)),
        Box::new(CausalityFeatureExtractor::pre(options.causality_dictionary_size())),
        Box::new(CausalityFeatureExtractor::post(options.causality_dictionary_size())),
        Box::new(ControlFlowFeaturesExtractor::new()),
      ],
    }
  }

  pub fn extractors_for_target<'ctx>(
    target: &String,
    target_type: FunctionType<'ctx>,
    options: &impl FeatureExtractorOptions,
  ) -> Self {
    Self {
      extractors: Self::all(options)
        .extractors
        .into_iter()
        .filter(|extractor| extractor.filter(target, target_type))
        .collect(),
    }
  }

  pub fn initialize(&mut self, slice: &Slice, num_traces: usize, trace: &Trace) {
    for extractor in &mut self.extractors {
      extractor.init(slice, num_traces, trace);
    }
  }

  pub fn finalize(&mut self) {
    for extractor in &mut self.extractors {
      extractor.finalize();
    }
  }

  pub fn extract_features(&self, slice: &Slice, trace: &Trace) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for extractor in &self.extractors {
      map.insert(extractor.name(), extractor.extract(&slice, &trace));
    }
    serde_json::Value::Object(map)
  }
}

pub struct FeatureExtractionContext<'a, 'ctx, O>
where
  O: FeatureExtractorOptions + IOOptions,
{
  pub modules: &'a Module<'ctx>,
  pub options: &'a O,
  pub target_num_slices_map: HashMap<String, usize>,
  pub func_types: HashMap<String, FunctionType<'ctx>>,
}

impl<'a, 'ctx, O> FeatureExtractionContext<'a, 'ctx, O>
where
  O: FeatureExtractorOptions + IOOptions,
{
  pub fn new(
    module: &'a Module<'ctx>,
    target_num_slices_map: HashMap<String, usize>,
    options: &'a O,
  ) -> Result<Self, String> {
    let func_types = module.function_types();
    Ok(Self {
      modules: module,
      options,
      target_num_slices_map,
      func_types,
    })
  }

  pub fn load_slices(&self, target: &String, num_slices: usize) -> Vec<Slice> {
    (0..num_slices)
      .collect::<Vec<_>>()
      .into_par_iter()
      .map(|slice_id| {
        let path = self.options.slice_target_file_path(target.as_str(), slice_id);
        load_json_t(&path).expect("Cannot load slice files")
      })
      .collect::<Vec<_>>()
  }

  pub fn load_trace_file_paths(&self, target: &String, slice_id: usize) -> Vec<(usize, PathBuf)> {
    fs::read_dir(self.options.trace_target_slice_dir(target.as_str(), slice_id))
      .expect("Cannot read traces folder")
      .map(|path| {
        let path = path.expect("Cannot read traces folder path").path();
        let trace_id = path.file_stem().unwrap().to_str().unwrap().parse::<usize>().unwrap();
        (trace_id, path)
      })
      .collect::<Vec<_>>()
  }

  pub fn load_trace(&self, path: &PathBuf) -> Trace {
    load_json_t(path).expect("Cannot load trace file")
  }

  pub fn extract_features(&self) {
    fs::create_dir_all(self.options.feature_dir()).expect("Cannot create features directory");

    self.target_num_slices_map.par_iter().for_each(|(target, &num_slices)| {
      // Initialize extractors
      let func_type = self.func_types[target];
      let mut extractors = FeatureExtractors::extractors_for_target(&target, func_type, self.options);

      // Load slices
      let slices = self.load_slices(&target, num_slices);

      // Initialize while loading traces
      (0..num_slices).for_each(|slice_id| {
        let slice = &slices[slice_id];
        let traces = self
          .load_trace_file_paths(&target, slice_id)
          .into_par_iter()
          .map(|(_, dir_entry)| self.load_trace(&dir_entry))
          .collect::<Vec<_>>();
        let num_traces = traces.len();
        for trace in traces {
          extractors.initialize(slice, num_traces, &trace);
        }
      });

      // Finalize feature extractor initialization
      extractors.finalize();

      // Extract features
      slices.par_iter().enumerate().for_each(|(slice_id, slice)| {
        // First create directory
        fs::create_dir_all(self.options.feature_target_slice_dir(target.as_str(), slice_id))
          .expect("Cannot create features target slice directory");

        // Then load trace file directories
        self
          .load_trace_file_paths(&target, slice_id)
          .into_par_iter()
          .for_each(|(trace_id, dir_entry)| {
            // Load trace json
            let trace = self.load_trace(&dir_entry);

            // Extract and dump features
            let features = extractors.extract_features(slice, &trace);
            let path = self
              .options
              .feature_target_slice_file_path(target.as_str(), slice_id, trace_id);
            dump_json(&features, path).expect("Cannot dump features json");
          })
      });
    });
  }
}
