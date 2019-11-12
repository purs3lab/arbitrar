let debug = ref false

let outdir = ref "extractor-out"

let slice_depth = ref 5

let target_function_name = ref ""

(* let start = ref 0

let amount = ref -1 *)

let common_opt =
  [ ("-debug", Arg.Set debug, "Enable debug mode")
  ; ("-outdir", Arg.Set_string outdir, "Output directory") ]

let slicer_opt =
  [ ("-n", Arg.Set_int slice_depth, "Code slicing depth")
  ; ("-fn", Arg.Set_string target_function_name, "Target function name") ]

(* ; ("-start", Arg.Set_int start, "Start index of slices")
  ; ("-amount", Arg.Set_int amount, "Amount of slices") ] *)

let slicer_opts = common_opt @ slicer_opt

let executor_opts = common_opt

let extractor_opts = common_opt @ slicer_opt

let options = ref extractor_opts
