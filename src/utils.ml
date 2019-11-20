exception InvalidJSON

exception NotImplemented

let string_of_instr instr = Llvm.string_of_llvalue instr |> String.trim

let string_of_lhs instr =
  let s = string_of_instr instr in
  let r = Str.regexp " = " in
  try
    let idx = Str.search_forward r s 0 in
    String.sub s 0 idx
  with Not_found ->
    prerr_endline ("Cannot find lhs of" ^ s) ;
    raise Not_found

let is_assignment = function
  | Llvm.Opcode.Invoke
  | Invalid2
  | Add
  | FAdd
  | Sub
  | FSub
  | Mul
  | FMul
  | UDiv
  | SDiv
  | FDiv
  | URem
  | SRem
  | FRem
  | Shl
  | LShr
  | AShr
  | And
  | Or
  | Xor
  | Alloca
  | Load
  | GetElementPtr
  | Trunc
  | ZExt
  | SExt
  | FPToUI
  | FPToSI
  | UIToFP
  | SIToFP
  | FPTrunc
  | FPExt
  | PtrToInt
  | IntToPtr
  | BitCast
  | ICmp
  | FCmp
  | PHI
  | Select
  | UserOp1
  | UserOp2
  | VAArg
  | ExtractElement
  | InsertElement
  | ShuffleVector
  | ExtractValue
  | InsertValue
  | Call (* FIXME: void *)
  | LandingPad ->
      true
  | _ ->
      false

let string_of_exp exp =
  match Llvm.classify_value exp with
  | Llvm.ValueKind.NullValue ->
      "0"
  | BasicBlock
  | InlineAsm
  | MDNode
  | MDString
  | BlockAddress
  | ConstantAggregateZero
  | ConstantArray
  | ConstantDataArray
  | ConstantDataVector
  | ConstantExpr ->
      Llvm.string_of_llvalue exp
  | Argument | ConstantFP | ConstantInt ->
      let s = string_of_instr exp in
      let r = Str.regexp " " in
      let idx = Str.search_forward r s 0 in
      String.sub s (idx + 1) (String.length s - idx - 1)
  | ConstantPointerNull ->
      "0"
  | ConstantStruct | ConstantVector ->
      Llvm.string_of_llvalue exp
  | Function ->
      Llvm.value_name exp
  | GlobalIFunc | GlobalAlias ->
      Llvm.string_of_llvalue exp
  | GlobalVariable ->
      Llvm.value_name exp
  | UndefValue ->
      "undef"
  | Instruction i when is_assignment i ->
      string_of_lhs exp
  | Instruction _ ->
      string_of_instr exp

let fold_left_all_instr f a m =
  Llvm.fold_left_functions
    (fun a func ->
      if Llvm.is_declaration func then a
      else
        Llvm.fold_left_blocks
          (fun a blk -> Llvm.fold_left_instrs (fun a instr -> f a instr) a blk)
          a func)
    a m

let json_of_opcode name =
  let opcode = ("opcode", `String name) in
  `Assoc [opcode]

let json_of_unop instr name =
  let opcode = ("opcode", `String name) in
  let result = `String (string_of_lhs instr) in
  let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
  `Assoc [opcode; ("result", result); ("op0", op0)]

let json_of_binop instr name =
  let opcode = ("opcode", `String name) in
  let result = `String (string_of_lhs instr) in
  let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
  let op1 = `String (string_of_exp (Llvm.operand instr 1)) in
  `Assoc [opcode; ("result", result); ("op0", op0); ("op1", op1)]

let json_of_icmp = function
  | Llvm.Icmp.Eq ->
      `String "eq"
  | Ne ->
      `String "ne"
  | Ugt ->
      `String "ugt"
  | Uge ->
      `String "uge"
  | Ult ->
      `String "ult"
  | Ule ->
      `String "ule"
  | Sgt ->
      `String "sgt"
  | Sge ->
      `String "sge"
  | Slt ->
      `String "slt"
  | Sle ->
      `String "sle"

let json_of_fcmp = function
  | Llvm.Fcmp.False ->
      `String "false"
  | Oeq ->
      `String "oeq"
  | Ogt ->
      `String "ogt"
  | Oge ->
      `String "oge"
  | Olt ->
      `String "olt"
  | Ole ->
      `String "ole"
  | One ->
      `String "one"
  | Ord ->
      `String "ord"
  | Uno ->
      `String "uno"
  | Ueq ->
      `String "ueq"
  | Ugt ->
      `String "ugt"
  | Uge ->
      `String "uge"
  | Ult ->
      `String "ult"
  | Ule ->
      `String "ule"
  | Une ->
      `String "une"
  | True ->
      `String "true"

let json_of_instr instr =
  let num_of_operands = Llvm.num_operands instr in
  match Llvm.instr_opcode instr with
  | Llvm.Opcode.Invalid ->
      json_of_opcode "invalid"
  | Invalid2 ->
      json_of_opcode "invalid2"
  | Unreachable ->
      json_of_opcode "unreachable"
  | Ret ->
      let opcode = ("opcode", `String "ret") in
      let ret =
        if num_of_operands = 0 then `Null
        else `String (string_of_exp (Llvm.operand instr 0))
      in
      `Assoc [opcode; ("op0", ret)]
  | Br ->
      let opcode = ("opcode", `String "br") in
      let cond =
        match Llvm.get_branch instr with
        | Some (`Conditional _) ->
            `String (string_of_exp (Llvm.operand instr 0))
        | _ ->
            `Null
      in
      `Assoc [opcode; ("cond", cond)]
  | Switch ->
      let opcode = ("opcode", `String "switch") in
      let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
      `Assoc [opcode; ("op0", op0)]
  | IndirectBr ->
      json_of_opcode "indirectbr"
  | Invoke ->
      json_of_opcode "invoke"
  | Add ->
      json_of_binop instr "add"
  | FAdd ->
      json_of_binop instr "fadd"
  | Sub ->
      json_of_binop instr "sub"
  | FSub ->
      json_of_binop instr "fsub"
  | Mul ->
      json_of_binop instr "mul"
  | FMul ->
      json_of_binop instr "fmul"
  | UDiv ->
      json_of_binop instr "udiv"
  | SDiv ->
      json_of_binop instr "sdiv"
  | FDiv ->
      json_of_binop instr "fdiv"
  | URem ->
      json_of_binop instr "urem"
  | SRem ->
      json_of_binop instr "srem"
  | FRem ->
      json_of_binop instr "frem"
  | Shl ->
      json_of_binop instr "shl"
  | LShr ->
      json_of_binop instr "lshr"
  | AShr ->
      json_of_binop instr "ashr"
  | And ->
      json_of_binop instr "and"
  | Or ->
      json_of_binop instr "or"
  | Xor ->
      json_of_binop instr "xor"
  | Alloca ->
      json_of_unop instr "alloca"
  | Load ->
      json_of_unop instr "load"
  | Store ->
      let opcode = ("opcode", `String "store") in
      let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
      let op1 = `String (string_of_exp (Llvm.operand instr 1)) in
      `Assoc [opcode; ("op0", op0); ("op1", op1)]
  | GetElementPtr ->
      let opcode = ("opcode", `String "getelementptr") in
      let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
      let result = `String (string_of_lhs instr) in
      `Assoc [opcode; ("op0", op0); ("result", result)]
  | Trunc ->
      json_of_unop instr "trunc"
  | ZExt ->
      json_of_unop instr "zext"
  | SExt ->
      json_of_unop instr "sext"
  | FPToUI ->
      json_of_unop instr "fptoui"
  | FPToSI ->
      json_of_unop instr "fptosi"
  | UIToFP ->
      json_of_unop instr "uitofp"
  | SIToFP ->
      json_of_unop instr "sitofp"
  | FPTrunc ->
      json_of_unop instr "fptrunc"
  | FPExt ->
      json_of_unop instr "fpext"
  | PtrToInt ->
      json_of_unop instr "ptrtoint"
  | IntToPtr ->
      json_of_unop instr "inttoptr"
  | BitCast ->
      json_of_unop instr "bitcast"
  | ICmp ->
      let opcode = ("opcode", `String "icmp") in
      let op =
        match Llvm.icmp_predicate instr with
        | Some s ->
            s
        | None ->
            failwith "Stmt.json_of_stmt (icmp)"
      in
      let predicate = ("predicate", json_of_icmp op) in
      let result = `String (string_of_lhs instr) in
      let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
      let op1 = `String (string_of_exp (Llvm.operand instr 1)) in
      `Assoc [opcode; ("result", result); predicate; ("op0", op0); ("op1", op1)]
  | FCmp ->
      let opcode = ("opcode", `String "fcmp") in
      let op =
        match Llvm.fcmp_predicate instr with
        | Some s ->
            s
        | None ->
            failwith "Stmt.json_of_stmt (fcmp)"
      in
      let predicate = ("predicate", json_of_fcmp op) in
      let result = `String (string_of_lhs instr) in
      let op0 = `String (string_of_exp (Llvm.operand instr 0)) in
      let op1 = `String (string_of_exp (Llvm.operand instr 1)) in
      `Assoc [opcode; ("result", result); predicate; ("op0", op0); ("op1", op1)]
  | PHI ->
      let opcode = ("opcode", `String "phi") in
      let result = `String (string_of_lhs instr) in
      let incoming =
        Llvm.incoming instr
        |> List.map (fun x -> `String (fst x |> string_of_exp))
      in
      `Assoc [opcode; ("result", result); ("incoming", `List incoming)]
  | Call ->
      let opcode = ("opcode", `String "call") in
      let result =
        match Llvm.type_of instr |> Llvm.classify_type with
        | Llvm.TypeKind.Void ->
            `Null
        | _ ->
            `String (string_of_lhs instr)
      in
      let callee =
        `String (Llvm.operand instr (num_of_operands - 1) |> string_of_exp)
      in
      let args =
        List.fold_left
          (fun args a -> args @ [`String (string_of_exp a)])
          []
          (List.init (num_of_operands - 1) (fun i -> Llvm.operand instr i))
      in
      `Assoc
        [opcode; ("result", result); ("func", callee); ("args", `List args)]
  | Select ->
      json_of_opcode "select"
  | UserOp1 ->
      json_of_opcode "userop1"
  | UserOp2 ->
      json_of_opcode "userop2"
  | VAArg ->
      json_of_opcode "vaarg"
  | ExtractElement ->
      json_of_opcode "extractelement"
  | InsertElement ->
      json_of_opcode "insertelement"
  | ShuffleVector ->
      json_of_opcode "shufflevector"
  | ExtractValue ->
      json_of_opcode "extractvalue"
  | InsertValue ->
      json_of_opcode "insertvalue"
  | Fence ->
      json_of_opcode "fence"
  | AtomicCmpXchg ->
      json_of_opcode "atomiccmpxchg"
  | AtomicRMW ->
      json_of_opcode "atomicrmw"
  | Resume ->
      json_of_opcode "resume"
  | LandingPad ->
      json_of_opcode "landingpad"
  | AddrSpaceCast ->
      json_of_opcode "addrspacecast"
  | CleanupRet ->
      json_of_opcode "cleanupret"
  | CatchRet ->
      json_of_opcode "catchret"
  | CatchPad ->
      json_of_opcode "catchpad"
  | CleanupPad ->
      json_of_opcode "cleanuppad"
  | CatchSwitch ->
      json_of_opcode "catchswitch"

let is_llvm_function f : bool =
  let r1 = Str.regexp "llvm\\.dbg\\..+" in
  let r2 = Str.regexp "llvm\\.lifetime\\..+" in
  Str.string_match r1 (Llvm.value_name f) 0
  || Str.string_match r2 (Llvm.value_name f) 0

let get_abs_path (name : string) =
  let is_starting_from_root = name.[0] = '/' in
  if is_starting_from_root then name else Filename.concat (Sys.getcwd ()) name

let rec unique (f : 'a -> 'a -> bool) (ls : 'a list) : 'a list =
  match ls with
  | hd :: tl ->
      let tl_no_hd = List.filter (fun x -> not (f hd x)) tl in
      let uniq_rest = unique f tl_no_hd in
      hd :: uniq_rest
  | [] ->
      []

let rec without (f : 'a -> bool) (ls : 'a list) : 'a list =
  match ls with
  | [] ->
      []
  | hd :: tl ->
      if f hd then without f tl else hd :: without f tl

let get_function_in_llm (func_name : string) (llm : Llvm.llmodule) :
    Llvm.llvalue =
  match Llvm.lookup_function func_name llm with
  | Some entry ->
      entry
  | None ->
      raise InvalidJSON

let get_field json field : Yojson.Safe.t =
  match json with
  | `Assoc fields -> (
    match List.find_opt (fun (key, _) -> key = field) fields with
    | Some (_, field_data) ->
        field_data
    | None ->
        raise InvalidJSON )
  | _ ->
      raise InvalidJSON

let get_field_opt json field : Yojson.Safe.t option =
  match json with
  | `Assoc fields -> (
    match List.find_opt (fun (key, _) -> key = field) fields with
    | Some (_, field_data) ->
        Some field_data
    | None ->
        None )
  | _ ->
      raise InvalidJSON

let string_from_json json : string =
  match json with `String str -> str | _ -> raise InvalidJSON

let string_opt_from_json json : string option =
  match json with `String str -> Some str | _ -> None

let int_from_json json : int =
  match json with `Int i -> i | _ -> raise InvalidJSON

let int_from_json_field json field : int = int_from_json (get_field json field)

let list_from_json json : Yojson.Safe.t list =
  match json with `List ls -> ls | _ -> raise InvalidJSON

let string_from_json_field json field : string =
  string_from_json (get_field json field)

let string_opt_from_json_field json field : string option =
  Option.bind (get_field_opt json field) string_opt_from_json

let string_list_from_json json : string list =
  match json with
  | `List ls ->
      List.map string_from_json ls
  | _ ->
      raise InvalidJSON
