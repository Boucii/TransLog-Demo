#!/usr/bin/env python3
from __future__ import annotations

import argparse
import itertools
import os
from dataclasses import dataclass
from typing import Dict, Iterable, List, Optional, Sequence, Set, Tuple

VARS = ["a", "b", "c", "d"]
VAR_COUNT = 4
INPUT_COUNT = 1 << VAR_COUNT


def truth_table_string(bits: int) -> str:
    return "".join("1" if (bits >> i) & 1 else "0" for i in range(INPUT_COUNT))


def eval_table(bits: int, a: int, b: int, c: int, d: int) -> int:
    idx = (a << 3) | (b << 2) | (c << 1) | d
    return (bits >> idx) & 1


def permute_table(bits: int, perm: Sequence[int]) -> int:
    out = 0
    for idx in range(INPUT_COUNT):
        a = (idx >> 3) & 1
        b = (idx >> 2) & 1
        c = (idx >> 1) & 1
        d = idx & 1
        v = [a, b, c, d]
        pa, pb, pc, pd = (v[perm[0]], v[perm[1]], v[perm[2]], v[perm[3]])
        if eval_table(bits, pa, pb, pc, pd):
            out |= 1 << idx
    return out


def canonical_table(bits: int, perms: Sequence[Sequence[int]]) -> int:
    best = None
    for perm in perms:
        candidate = permute_table(bits, perm)
        if best is None or candidate < best:
            best = candidate
    return best if best is not None else bits


@dataclass(frozen=True)
class Implicant:
    bits: int
    mask: int

    def covers(self, minterm: int) -> bool:
        return (minterm & ~self.mask) == (self.bits & ~self.mask)

    def literals(self) -> int:
        return VAR_COUNT - popcount(self.mask)


def popcount(x: int) -> int:
    return bin(x).count("1")


def combine_implicants(a: Implicant, b: Implicant) -> Optional[Implicant]:
    if a.mask != b.mask:
        return None
    diff = (a.bits ^ b.bits) & ~a.mask
    if popcount(diff) != 1:
        return None
    new_mask = a.mask | diff
    new_bits = a.bits & ~diff
    return Implicant(new_bits, new_mask)


def prime_implicants(minterms: List[int]) -> List[Implicant]:
    current = {Implicant(m, 0) for m in minterms}
    primes: Set[Implicant] = set()
    while current:
        next_round: Set[Implicant] = set()
        used: Set[Implicant] = set()
        buckets: Dict[Tuple[int, int], List[Implicant]] = {}
        for imp in current:
            ones = popcount(imp.bits & ~imp.mask)
            buckets.setdefault((imp.mask, ones), []).append(imp)
        for (mask, ones), group in buckets.items():
            neighbor = buckets.get((mask, ones + 1), [])
            for a in group:
                for b in neighbor:
                    combined = combine_implicants(a, b)
                    if combined is not None:
                        used.add(a)
                        used.add(b)
                        next_round.add(combined)
        for imp in current:
            if imp not in used:
                primes.add(imp)
        current = next_round
    return sorted(primes, key=lambda x: (x.mask, x.bits))


def essential_and_remaining(
    primes: List[Implicant], minterms: List[int]
) -> Tuple[List[Implicant], List[Implicant], int]:
    minterm_to_primes: Dict[int, List[int]] = {m: [] for m in minterms}
    for i, imp in enumerate(primes):
        for m in minterms:
            if imp.covers(m):
                minterm_to_primes[m].append(i)
    essentials: List[Implicant] = []
    essential_indices: Set[int] = set()
    covered_mask = 0
    for m, indices in minterm_to_primes.items():
        if len(indices) == 1:
            idx = indices[0]
            if idx not in essential_indices:
                essential_indices.add(idx)
                essentials.append(primes[idx])
    for m in minterms:
        for imp in essentials:
            if imp.covers(m):
                covered_mask |= 1 << m
                break
    remaining = [imp for i, imp in enumerate(primes) if i not in essential_indices]
    return essentials, remaining, covered_mask


def minimize_sop(minterms: List[int]) -> List[Implicant]:
    if not minterms:
        return []
    if len(minterms) == INPUT_COUNT:
        return [Implicant(0, (1 << VAR_COUNT) - 1)]

    primes = prime_implicants(minterms)
    essentials, remaining, covered_mask = essential_and_remaining(primes, minterms)
    target_mask = 0
    for m in minterms:
        target_mask |= 1 << m
    uncovered = target_mask & ~covered_mask
    if uncovered == 0:
        return essentials

    remaining_covers = []
    for imp in remaining:
        cover = 0
        for m in minterms:
            if imp.covers(m):
                cover |= 1 << m
        remaining_covers.append(cover)

    best: Optional[Tuple[int, int, List[int]]] = None

    def cost(implicants: List[Implicant]) -> Tuple[int, int]:
        return (len(implicants), sum(imp.literals() for imp in implicants))

    def dfs(current_uncovered: int, chosen: List[int]) -> None:
        nonlocal best
        if current_uncovered == 0:
            selected = [remaining[i] for i in chosen]
            total = essentials + selected
            c = cost(total)
            if best is None or c < (best[0], best[1]):
                best = (c[0], c[1], chosen.copy())
            return

        if best is not None:
            if len(essentials) + len(chosen) >= best[0]:
                return

        minterm = None
        min_choices = None
        for m in range(INPUT_COUNT):
            if (current_uncovered >> m) & 1:
                choices = [i for i, cover in enumerate(remaining_covers) if (cover >> m) & 1]
                if not choices:
                    return
                if min_choices is None or len(choices) < len(min_choices):
                    minterm = m
                    min_choices = choices
        if min_choices is None:
            return

        for i in min_choices:
            if i in chosen:
                continue
            new_uncovered = current_uncovered & ~remaining_covers[i]
            chosen.append(i)
            dfs(new_uncovered, chosen)
            chosen.pop()

    dfs(uncovered, [])
    if best is None:
        return essentials
    best_selected = [remaining[i] for i in best[2]]
    return essentials + best_selected


def implicant_to_expr(imp: Implicant) -> str:
    if imp.mask == (1 << VAR_COUNT) - 1:
        return "1"
    parts = []
    for i, var in enumerate(VARS):
        bit = (imp.bits >> (VAR_COUNT - 1 - i)) & 1
        mask_bit = (imp.mask >> (VAR_COUNT - 1 - i)) & 1
        if mask_bit:
            continue
        parts.append(var if bit else f"!{var}")
    return "&".join(parts) if parts else "1"


def expr_for_table(bits: int) -> str:
    minterms = [i for i in range(INPUT_COUNT) if (bits >> i) & 1]
    if not minterms:
        return "0"
    if len(minterms) == INPUT_COUNT:
        return "1"
    implicants = minimize_sop(minterms)
    terms = [implicant_to_expr(imp) for imp in implicants]
    return " + ".join(terms)


def generate_dataset() -> List[Tuple[int, int, int, str]]:
    perms = list(itertools.permutations(range(VAR_COUNT), VAR_COUNT))
    classes: Dict[int, int] = {}
    for bits in range(1 << INPUT_COUNT):
        canon = canonical_table(bits, perms)
        if canon not in classes:
            classes[canon] = bits

    if len(classes) != 3984:
        raise RuntimeError(f"Expected 3984 classes, found {len(classes)}")

    dataset = []
    for canon in sorted(classes.keys()):
        rep = classes[canon]
        expr = expr_for_table(rep)
        dataset.append((rep, canon, canon, expr))
    return dataset


def write_output(path: str, dataset: List[Tuple[int, int, int, str]]) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        for idx, (rep, canon, _, expr) in enumerate(dataset):
            f.write(f"ID: {idx}\n")
            f.write(f"TruthTable: {truth_table_string(rep)}\n")
            f.write(f"Canonical: {truth_table_string(canon)}\n")
            f.write(f"Expr: {expr}\n\n")


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate 4-input P-class dataset.")
    parser.add_argument("--out", required=True, help="Output file path")
    args = parser.parse_args()

    dataset = generate_dataset()
    write_output(args.out, dataset)


if __name__ == "__main__":
    main()
