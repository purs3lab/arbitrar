use llir::{types::*, Module};
use rayon::prelude::*;
use serde::{Deserialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
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

impl Trace {
  pub fn target_result(&self) -> &Option<Value> {
    &self.instrs[self.target].res
  }
}

pub trait FeatureExtractor: Send + Sync {
  fn name(&self) -> String;

  fn filter<'ctx>(&self, target: &String, target_type: FunctionType<'ctx>) -> bool;

  fn init(&mut self, slice: &Slice, num_traces: usize, trace: &Trace);

  fn finalize(&mut self);

  fn extract(&self, slice: &Slice, trace: &Trace) -> serde_json::Value;
}

pub struct FeatureExtractors {
  extractors: Vec<Box<dyn FeatureExtractor>>
}

impl FeatureExtractors {
  fn all(options: &Options) -> Self {
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
        Box::new(CausalityFeatureExtractor::pre(options.causality_dictionary_size)),
        Box::new(CausalityFeatureExtractor::post(options.causality_dictionary_size)),
        Box::new(LoopFeaturesExtractor::new()),
      ]
    }
  }

  fn extractors_for_target<'ctx>(target: &String, target_type: FunctionType<'ctx>, options: &Options) -> Self {
    Self {
      extractors: Self::all(options)
        .extractors
        .into_iter()
        .filter(|extractor| extractor.filter(target, target_type))
        .collect()
    }
  }

  fn initialize(&mut self, slice: &Slice, num_traces: usize, trace: &Trace) {
    for extractor in &mut self.extractors {
      extractor.init(slice, num_traces, trace);
    }
  }

  fn finalize(&mut self) {
    for extractor in &mut self.extractors {
      extractor.finalize();
    }
  }

  fn extract_features(&self, slice: &Slice, trace: &Trace) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for extractor in &self.extractors {
      map.insert(extractor.name(), extractor.extract(&slice, &trace));
    }
    serde_json::Value::Object(map)
  }
}

pub struct FeatureExtractionContext<'a, 'ctx> {
  pub module: &'a Module<'ctx>,
  pub options: &'a Options,
  pub target_num_slices_map: HashMap<String, usize>,
  pub func_types: HashMap<String, FunctionType<'ctx>>,
}

impl<'a, 'ctx> FeatureExtractionContext<'a, 'ctx> {
  pub fn new(
    module: &'a Module<'ctx>,
    target_num_slices_map: HashMap<String, usize>,
    options: &'a Options,
  ) -> Result<Self, String> {
    let func_types = module.function_types();
    Ok(Self {
      module,
      options,
      target_num_slices_map,
      func_types,
    })
  }

  pub fn load_slices(&self, target: &String, num_slices: usize) -> Vec<Slice> {
    (0..num_slices)
      .collect::<Vec<_>>()
      .par_iter()
      .map(|slice_id| {
        let path = self.options.slice_file_path(target.as_str(), *slice_id);
        let file = File::open(path).expect("Could not open slice file");
        serde_json::from_reader(file).expect("Cannot parse slice file")
      })
      .collect::<Vec<_>>()
  }

  pub fn load_trace_file_paths(&self, target: &String, slice_id: usize) -> Vec<PathBuf> {
    fs::read_dir(self.options.trace_target_slice_dir_path(target.as_str(), slice_id))
      .expect("Cannot read traces folder")
      .map(|path| path.expect("Cannot read traces folder path").path())
      .collect::<Vec<_>>()
  }

  pub fn load_trace(&self, path: &PathBuf) -> Trace {
    let trace_file = File::open(path).expect("Could not open trace file");
    serde_json::from_reader(trace_file).expect("Cannot parse trace file")
  }

  pub fn dump_features(&self, features: &serde_json::Value, target: &String, slice_id: usize, trace_id: usize) {
    let features_str = serde_json::to_string(&features).expect("Cannot stringify features json");
    let mut features_file = File::create(self.options.features_file_path(target.as_str(), slice_id, trace_id))
      .expect("Cannot create features file");
    features_file
      .write_all(features_str.as_bytes())
      .expect("Cannot write to features file");
  }

  pub fn extract_features(&self) {
    fs::create_dir_all(self.options.features_dir_path()).expect("Cannot create features directory");

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
          .par_iter()
          .map(|dir_entry| -> Trace {
            let file = File::open(dir_entry).expect("Could not open trace file");
            serde_json::from_reader(file).expect("Cannot parse trace file")
          })
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
        fs::create_dir_all(self.options.features_target_slice_dir_path(target.as_str(), slice_id))
          .expect("Cannot create features target slice directory");

        // Then load trace file directories
        self
          .load_trace_file_paths(&target, slice_id)
          .par_iter()
          .enumerate()
          .for_each(|(trace_id, dir_entry)| {
            // Load trace json
            let trace = self.load_trace(dir_entry);

            // Extract and dump features
            let features = extractors.extract_features(slice, &trace);
            self.dump_features(&features, &target, slice_id, trace_id);
          })
      });
    });
  }
}
