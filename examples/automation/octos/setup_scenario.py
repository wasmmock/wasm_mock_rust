#!/usr/bin/env python3
"""Install one of gen_scenario.py's scenarios into this directory, by name.

    python3 examples/automation/octos/setup_scenario.py hydrate-no-messages
    bash cli/mock_octos.sh

The files here are the mock's data: `build.rs` bakes whatever is in this
directory into the guest, so "run scenario N" means "put scenario N's JSON here
and rebuild". This script is that step — it restores the recorded baseline and
drops one scenario's overrides on top, so no scenario can inherit the last one's
mutations.

    --list [text]   scenarios, optionally filtered by name or area
    --show NAME     what a scenario changes and what it expects, without installing
    --current       what is installed here now
    --restore       put the recorded baseline back
    --seed          take the baseline copy (done automatically, once)

A scenario can be named or numbered; the number is the scenario_id a campaign's
results are keyed on, so `137` and `turn-mismatched-segment-id` are the same
case.
"""
import argparse, json, os, shutil, sys

HERE = os.path.dirname(os.path.abspath(__file__))
# The untouched capture, taken once by --seed and then committed. It has to be a
# copy rather than "whatever is in this directory": installing a scenario
# overwrites the data set in place, so the only other way back is to remember
# which files the last scenario touched. 68 KB is cheaper than a baseline nobody
# can recover.
PRISTINE = os.path.join(HERE, "pristine")
HEARTBEAT = "server_heartbeat.json"


def installed():
    """The heartbeat as it stands here: which scenario this data set is."""
    try:
        with open(os.path.join(HERE, HEARTBEAT), encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, ValueError):
        return {}


def data_files():
    """Every response body named by index.json, plus the heartbeat.

    Read from the index rather than globbed: a stray JSON in this directory is
    not part of the data set, and copying it into the baseline would quietly
    make it one.
    """
    with open(os.path.join(HERE, "index.json"), encoding="utf-8") as handle:
        index = json.load(handle)
    names = [entry["file"] for entry in index["results"]]
    names.append(index["turn_stream"]["file"])
    return sorted(set(names))


def seed(force=False):
    """Copy the current data set aside as the baseline, once.

    Guarded on the heartbeat rather than on the directory being untouched: only
    the pristine data set says `"scenario": "pristine"`, so baselining a
    half-installed scenario — which would silently poison every scenario built
    from it afterwards — is refused rather than merely discouraged.
    """
    if os.path.isdir(PRISTINE) and not force:
        return False
    scenario = installed().get("scenario")
    if scenario != "pristine" and not force:
        raise SystemExit(
            f"refusing to baseline: this data set is {scenario!r}, not 'pristine'.\n"
            "Get the baseline back with `git checkout examples/automation/octos/pristine`,\n"
            "then `--restore`. Pass --seed --force only to baseline what is here now.")
    os.makedirs(PRISTINE, exist_ok=True)
    for name in data_files() + [HEARTBEAT]:
        shutil.copy(os.path.join(HERE, name), os.path.join(PRISTINE, name))
    return True


def write(name, body):
    """Write a data file, stamped now.

    Never shutil.copy2, and never a plain copy that keeps the source mtime:
    cargo compares file mtimes against its build output, so a file older than
    the last build looks up to date and the rebuild is skipped. That once served
    one scenario's wasm for a whole campaign while every deploy reported ok.
    """
    path = os.path.join(HERE, name)
    with open(path, "w", encoding="utf-8") as handle:
        if isinstance(body, str):
            handle.write(body)
        else:
            json.dump(body, handle, ensure_ascii=False)
    os.utime(path, None)


def restore():
    """Put the baseline back, every file of it."""
    seed()
    for name in sorted(os.listdir(PRISTINE)):
        if name.endswith(".json"):
            with open(os.path.join(PRISTINE, name), encoding="utf-8") as handle:
                write(name, handle.read())


def catalogue():
    """gen_scenario.py's scenarios as (id, name, area, build).

    Imported late: the module loads the baseline at import time, so seeding has
    to have happened first.
    """
    seed()
    sys.path.insert(0, HERE)
    import gen_scenario

    gen_scenario.check_expectations()
    return gen_scenario, [(index, name, area, build)
                          for index, (name, area, build)
                          in enumerate(gen_scenario.SCENARIOS, start=1)]


def find(catalog, wanted):
    """A scenario by name or by scenario_id, or a message naming near misses."""
    if wanted.isdigit():
        for entry in catalog:
            if entry[0] == int(wanted):
                return entry
        raise SystemExit(f"no scenario {wanted}; ids run 1..{len(catalog)}")
    for entry in catalog:
        if entry[1] == wanted:
            return entry
    near = [name for _, name, _, _ in catalog if wanted in name]
    hint = "\n  ".join(near[:10]) if near else "(nothing similar; --list shows them all)"
    raise SystemExit(f"no scenario named {wanted!r}. Did you mean:\n  {hint}")


def describe(index, name, area, files, gen):
    """What this scenario changes, from its own heartbeat."""
    heartbeat = files["heartbeat"]
    lines = [f"{index:03} {name}  [{area}]", f"  expects: {heartbeat['expectation']}"]
    for entry in heartbeat["modified"]:
        lines.append(f"  {entry['file']} ({entry['method']})")
        for field in entry["fields"]:
            lines.append(f"    {field}")
        if entry["truncated"]:
            lines.append("    ... (truncated)")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__.split("\n")[0],
        formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("scenario", nargs="?", help="scenario name or id to install")
    parser.add_argument("--list", nargs="?", const="", metavar="TEXT",
                        help="list scenarios, optionally filtered by name or area")
    parser.add_argument("--show", metavar="NAME", help="describe a scenario without installing it")
    parser.add_argument("--current", action="store_true", help="what is installed here now")
    parser.add_argument("--restore", action="store_true", help="put the recorded baseline back")
    parser.add_argument("--seed", action="store_true", help="take the baseline copy")
    parser.add_argument("--force", action="store_true", help="with --seed, baseline whatever is here")
    args = parser.parse_args()

    if args.seed:
        print("baselined" if seed(args.force) else f"already baselined: {PRISTINE}")
        return

    if args.current:
        heartbeat = installed()
        if not heartbeat:
            raise SystemExit(f"no readable {HEARTBEAT} here")
        print(json.dumps(heartbeat, indent=1, ensure_ascii=False))
        return

    if args.restore:
        restore()
        print(f"restored the recorded baseline into {HERE}")
        print("rebuild with: bash cli/mock_octos.sh")
        return

    if args.list is not None:
        _, catalog = catalogue()
        text = args.list.lower()
        rows = [e for e in catalog if not text or text in e[1].lower() or text in e[2].lower()]
        for index, name, area, _ in rows:
            print(f"{index:03}  {area:12}  {name}")
        print(f"\n{len(rows)} of {len(catalog)} scenarios")
        return

    wanted = args.show or args.scenario
    if not wanted:
        parser.print_help()
        return

    gen, catalog = catalogue()
    index, name, area, build = find(catalog, wanted)
    files = gen.materialise(index, name, area, build)

    if args.show:
        print(describe(index, name, area, files, gen))
        return

    # Baseline first: a scenario overrides two or three files, and whatever the
    # last one changed would otherwise still be sitting in the others.
    restore()
    for key, value in files.items():
        write(gen.FILES[key], value)

    print(describe(index, name, area, files, gen))
    print("\ninstalled. Deploy with: bash cli/mock_octos.sh")


if __name__ == "__main__":
    main()
