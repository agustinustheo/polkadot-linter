#!/usr/bin/env python3
"""Independent, indentation/regex benchmark inventory for the pinned SDK.

This is evidence tooling, not imported by the linter. It deliberately does not
use syn, the linter's token parser, or linter output. Compiler expansion is the
cross-check for supported syntax; see benchmarks/labels/BEN-oracle.md.
"""
import argparse
import json
import re
from pathlib import Path


def dispatched_bindings(setup, measured):
    bindings = dict(re.findall(r'let\s+(\w+)\s*=\s*(?:\w*Call)\s*::\s*<[^>]*>\s*::\s*(\w+)\s*\{', setup))
    dispatched = re.findall(r'\b(\w+)\.dispatch(?:_bypass_filter)?\s*\(', measured)
    return [bindings[name] for name in dispatched if name in bindings]


def inventory(sdk):
    entries = []
    for path in sorted(sdk.rglob('*.rs')):
        if any(part in {'target', '.git', 'tests', 'examples'} for part in path.parts):
            continue
        source = path.read_text()
        package_dir = next((p for p in path.parents if (p / 'Cargo.toml').exists()), None)
        if package_dir is None:
            continue
        package = re.search(r'^name\s*=\s*"([^"]+)"', (package_dir / 'Cargo.toml').read_text(), re.M)
        if package is None:
            continue
        # v2: the fn closing brace has its declaration's indentation. This is
        # independent of nested braces and const generic expressions.
        pattern = r'^([ \t]*)#\[(?:\w+::)*benchmark(?:\([^\n]*\))?\]\s*(?:#\[[^\n]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)\b'
        for match in re.finditer(pattern, source, re.M):
            tail = source[match.end():]
            end = re.search(r'^' + re.escape(match[1]) + r'\}', tail, re.M)
            if end is None:
                continue
            body = tail[:end.end()]
            measured = re.search(r'#\[(?:\w+::)*extrinsic_call\]\s*([\w:]+)\s*\(', body)
            calls = []
            if measured:
                calls = [match[2] if measured[1] == '_' else measured[1].split('::')[-1]]
            else:
                block = re.search(r'#\[(?:\w+::)*block\]', body)
                if block:
                    calls = re.findall(r'::\s*(\w+)\s*\(', body[block.end():]) + dispatched_bindings(body[:block.start()], body[block.end():])
            entries.append(dict(package=package[1], path=str(path.relative_to(sdk)), line=source[:match.start()].count('\n')+1, name=match[2], calls=sorted(set(calls)), syntax='v2'))
        # v1: setup ends at a same-indentation `}:`; entry headers must be
        # within a legacy macro and may not be `verify` or `where_clause`.
        if re.search(r'\bbenchmarks(?:_instance(?:_pallet)?)?\s*!', source):
            for match in re.finditer(r'^([ \t]+)(\w+)\s*\{\s*$', source, re.M):
                if match[2] in {'verify', 'where_clause'}:
                    continue
                tail = source[match.end():]
                boundary = re.search(r'^' + re.escape(match[1]) + r'\}\s*:\s*', tail, re.M)
                # A nested block cannot cross the entry's closing boundary.
                close = re.search(r'^' + re.escape(match[1]) + r'\}', tail, re.M)
                if boundary is None or close is None or boundary.start() != close.start():
                    continue
                measured = tail[boundary.end():]
                call = re.match(r'(\w+)\s*(?:<[^\n]*>)?\s*\(', measured)
                calls = [match[2] if call[1] == '_' else call[1]] if call else []
                if measured.startswith('{'):
                    end = re.search(r'^' + re.escape(match[1]) + r'\}', measured, re.M)
                    measured_body = measured[:end.end()] if end else measured
                    calls = re.findall(r'::\s*(\w+)\s*\(', measured_body) + dispatched_bindings(tail[:boundary.start()], measured_body)
                entries.append(dict(package=package[1], path=str(path.relative_to(sdk)), line=source[:match.start()].count('\n')+1, name=match[2], calls=sorted(set(calls)), syntax='v1'))
    return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('sdk', type=Path)
    parser.add_argument('--expanded', type=Path, help='compiler expansion to compare against inventory')
    parser.add_argument('--package', help='package whose expansion is being checked')
    args = parser.parse_args()
    entries = inventory(args.sdk)
    if args.expanded:
        expanded = args.expanded.read_text()
        enums = re.findall(r'enum SelectedBenchmark\s*\{([^}]+)\}', expanded)
        names = set(re.findall(r'\b(\w+)\s*,', '\n'.join(enums)))
        actual = {e['name'] for e in entries if e['package'] == args.package}
        dispatchables = set(re.findall(r'fn new_call_variant_(\w+)\b', expanded))
        if not dispatchables:
            raise SystemExit('No generated dispatchable constructors found in expansion')
        call_checks = {}
        for implementation in re.finditer(r'BenchmarkingSetup<[^>]*>\s+for\s+(\w+)', expanded):
            name = implementation[1]
            if name not in actual:
                continue
            start = expanded.index('{', implementation.end())
            depth = 0
            end = start
            for token in re.finditer(r'"(?:\\.|[^"\\])*"|[{}]', expanded[start:]):
                if token[0] == '{':
                    depth += 1
                elif token[0] == '}':
                    depth -= 1
                    if depth == 0:
                        end = start + token.end()
                        break
            body = expanded[start:end]
            compiled_calls = set(re.findall(r'\bCall\s*::\s*<[^>]+>\s*::\s*(?:new_call_variant_)?(\w+)\s*[({]', body))
            measured = re.search(r'recording\.start\(\);(.*?)recording\.stop\(\)', body, re.S)
            if measured:
                compiled_calls.update(re.findall(r'::\s*(\w+)\s*\(', measured[1]))
                # Generated dispatch scaffolding is not an authored measured call.
                compiled_calls -= {'dispatch_bypass_filter', 'from'}
            expected_calls = {call for e in entries if e['package'] == args.package and e['name'] == name for call in e['calls']}
            compiled_calls &= dispatchables
            expected_calls &= dispatchables
            call_checks[name] = dict(compiler=sorted(compiled_calls), oracle=sorted(expected_calls), match=compiled_calls == expected_calls)
        print(json.dumps(dict(package=args.package, compiler=sorted(names), oracle=sorted(actual), missing=sorted(names-actual), extra=sorted(actual-names), measured_calls=call_checks), indent=2))
        if not names or names != actual or set(call_checks) != actual or any(not check['match'] for check in call_checks.values()):
            raise SystemExit(1)
    else:
        print(json.dumps(entries, indent=2))


if __name__ == '__main__':
    main()
