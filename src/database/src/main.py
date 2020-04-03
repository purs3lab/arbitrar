import pprint
pp = pprint.PrettyPrinter(indent=4)

def setup_parser(parser):
    subparsers = parser.add_subparsers(dest="query")

    packages = subparsers.add_parser("packages")

    bc_files_parser = subparsers.add_parser("bc-files")
    bc_files_parser.add_argument('-p', '--package', type=str, help="Only the bc files in a package")

    num_slices_parser = subparsers.add_parser("num-slices")
    num_slices_parser.add_argument('-p', '--package', type=str, help='Only the slices in a package')
    num_slices_parser.add_argument('-b', '--bc', type=str, help='Only the slices in a bc-file')
    num_slices_parser.add_argument('-f', '--function', type=str, help='Only the slices around a function')

    slice_parser = subparsers.add_parser("slice")
    slice_parser.add_argument('bc-file', type=str, help='The bc-file that the slice belongs to')
    slice_parser.add_argument('function', type=str, help='The function that the slice contain')
    slice_parser.add_argument('slice-id', type=int, help='The slice id')

    num_traces_parser = subparsers.add_parser("num-traces")
    num_traces_parser.add_argument('-p', '--package', type=str, help='Only the traces in a package')
    num_traces_parser.add_argument('-b', '--bc', type=str, help='Only the traces in a bc-file')
    num_traces_parser.add_argument('-f', '--function', type=str, help='Only the traces around a function')

    trace_parser = subparsers.add_parser("trace")
    trace_parser.add_argument('bc-file', type=str, help="The bc-file that the trace belong to")
    trace_parser.add_argument('function', type=str, help="The function that the trace is about")
    trace_parser.add_argument('slice-id', type=int, help='The slice id')
    trace_parser.add_argument('trace-id', type=int, help='The trace id')

    feature_parser = subparsers.add_parser("feature")
    feature_parser.add_argument('bc-file', type=str, help="The bc-file that the trace belong to")
    feature_parser.add_argument('function', type=str, help="The function that the trace is about")
    feature_parser.add_argument('slice-id', type=int, help='The slice id')
    feature_parser.add_argument('trace-id', type=int, help='The trace id')


def print_packages(args):
    db = args.db
    print("Name\tFetch Status\tBuild Status")
    for package in db.packages:
        f = "fetched" if package.fetched else "not fetched"
        b = package.build.result.value
        print(f"{package.name}\t{f}\t\t{b}")


def print_bc_files(args):
    bc_files = args.db.bc_files(package = args.package)
    for bc_file in bc_files:
        print(bc_file)


def print_num_slices(args):
    db = args.db
    print(db.num_slices(func_name = args.function, bc = bc_name))


def print_slice(args):
    db = args.db
    var_args = vars(args)
    input_bc_file = var_args["bc-file"]
    bc_name = args.db.find_bc_name(input_bc_file)
    if bc_name:
        pp.pprint(args.db.slice(args.function, bc_name, var_args["slice-id"]))
    else:
        print(f"Unknown bc {input_bc_file}")


def print_num_traces(args):
    raise Exception("Not implemented")


def print_trace(args):
    raise Exception("Not implemented")


def print_feature(args):
    raise Exception("Not implemented")

actions = {
    'packages': print_packages,
    'bc-files': print_bc_files,
    'num-slices': print_num_slices,
    'slice': print_slice,
    'num-traces': print_num_traces,
    'trace': print_trace,
    'feature': print_feature
}

def main(args):
    if args.query in actions:
        actions[args.query](args)
    else:
        print(f"Unknown query {args.query}")
