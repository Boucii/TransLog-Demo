#!/usr/bin/env python3
from __future__ import annotations

import argparse
from dataclasses import dataclass
from typing import List, Optional, Tuple


@dataclass
class Node:
    kind: str
    value: Optional[str] = None
    left: Optional["Node"] = None
    right: Optional["Node"] = None


def tokenize(expr: str) -> List[str]:
    tokens: List[str] = []
    i = 0
    expr = expr.replace(" ", "")
    while i < len(expr):
        ch = expr[i]
        if ch in ("!", "&", "+", "(", ")"):
            tokens.append(ch)
            i += 1
        elif ch.isalpha():
            tokens.append(ch)
            i += 1
        elif ch in ("0", "1"):
            tokens.append(ch)
            i += 1
        else:
            raise ValueError(f"Unexpected character: {ch}")
    return tokens


def precedence(op: str) -> int:
    if op == "!":
        return 3
    if op == "&":
        return 2
    if op == "+":
        return 1
    return 0


def is_right_associative(op: str) -> bool:
    return op == "!"


def apply_op(op: str, stack: List[Node]) -> None:
    if op == "!":
        if not stack:
            raise ValueError("Missing operand for !")
        child = stack.pop()
        stack.append(Node(kind="not", left=child))
        return
    if op in ("&", "+"):
        if len(stack) < 2:
            raise ValueError(f"Missing operands for {op}")
        right = stack.pop()
        left = stack.pop()
        kind = "and" if op == "&" else "or"
        stack.append(Node(kind=kind, left=left, right=right))
        return
    raise ValueError(f"Unknown operator: {op}")


def parse_expr(expr: str) -> Node:
    tokens = tokenize(expr)
    ops: List[str] = []
    vals: List[Node] = []

    for tok in tokens:
        if tok.isalpha():
            vals.append(Node(kind="var", value=tok))
        elif tok in ("0", "1"):
            vals.append(Node(kind="const", value=tok))
        elif tok == "(":
            ops.append(tok)
        elif tok == ")":
            while ops and ops[-1] != "(":
                apply_op(ops.pop(), vals)
            if not ops or ops[-1] != "(":
                raise ValueError("Mismatched parentheses")
            ops.pop()
        else:
            while ops and ops[-1] != "(":
                top = ops[-1]
                if is_right_associative(tok):
                    if precedence(top) > precedence(tok):
                        apply_op(ops.pop(), vals)
                    else:
                        break
                else:
                    if precedence(top) >= precedence(tok):
                        apply_op(ops.pop(), vals)
                    else:
                        break
            ops.append(tok)

    while ops:
        op = ops.pop()
        if op in ("(", ")"):
            raise ValueError("Mismatched parentheses")
        apply_op(op, vals)

    if len(vals) != 1:
        raise ValueError("Invalid expression")
    return vals[0]


def to_sexpr(node: Node) -> str:
    if node.kind == "var":
        return node.value or ""
    if node.kind == "const":
        return "true" if node.value == "1" else "false"
    if node.kind == "not":
        return f"(! {to_sexpr(node.left)})"
    if node.kind == "and":
        return f"(* {to_sexpr(node.left)} {to_sexpr(node.right)})"
    if node.kind == "or":
        return f"(+ {to_sexpr(node.left)} {to_sexpr(node.right)})"
    raise ValueError(f"Unknown node kind: {node.kind}")


def convert_file(input_path: str, output_path: str) -> None:
    outputs: List[str] = []
    with open(input_path, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line.startswith("Expr:"):
                continue
            expr = line[len("Expr:"):].strip()
            if expr == "":
                raise ValueError("Empty expression line")
            node = parse_expr(expr)
            outputs.append(to_sexpr(node))

    with open(output_path, "w", encoding="utf-8") as f:
        for s in outputs:
            f.write(s + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description="Convert Expr lines to TransLog S-expr.")
    parser.add_argument("--input", required=True, help="Input dataset file")
    parser.add_argument("--output", required=True, help="Output S-expression file")
    args = parser.parse_args()
    convert_file(args.input, args.output)


if __name__ == "__main__":
    main()
