## TransLog's Grammar and semantics
TransLog is derived from boolean logical expressions while added some structual operators.
The grammar of TransLog is as below:

$$
\begin{aligned}
Cdual ::= &\ join(N, N) \mid !(Cdual) \mid (CellNoX \& CellNoX) \\
Cvar  ::= &\ Var \\
Cbool ::= &\ Bool \\
CX    ::= &\ X(Var, Num) \\
Cell    ::= &\ Cdual \mid Cvar \mid Cbool \mid CX \\
CellNoX ::= &\ Cdual \mid Cvar \mid Cbool \\
N ::= &\ Cell \mid N + N \mid N * N \mid bridge(Cell, Cell, Cell, Cell, Cell) \\
L ::= &\ Cell \mid N \\
Num ::= &\ digit^+
\end{aligned}
$$

Table I explicitly shows 2 types of signals TransLog trying to model, with the type of signal each operator produces, and the types of signal their oprand requires.
table II defines semantics of operator. It also defines how to calculate transistor count of the operator.
The semantics and transistor count are used in translate to SPICE and count transistors.

there are 2 signal types:
cell: signal with both pull up and pull down capacitance. Or var or constant. 
network: signal either can pull up or pull down.

table I: operator, and signal type of operator, and requrired signal type for each op
| Operator            | Signal Type | Required Signal type for Ops           |
|---------------------|-------------|----------------------------------------|
| join (a,b)          | cell.dual   | network/cell                           |
| ! (x)               | cell.dual   | cell                                   |
| + (x,y)             | network     | cell/network                           |
| * (x,y)             | network     | cell/network                           |
| bridge(a,b,c,d,e)   | network     | cell/network                           |
| & (a,b)             | cell.dual   | cell.dual/cell.bool/cell.var           |
| X (a, size)         | cell.X      | a:cell.var b:decimal literal           |
| Bool                | cell.bool   |                                        |
| Var                 | cell.var    |                                        |

table II
| Operator | Semantics | Transistor Count |
|-----------------|-----------|------------------|
| join (a,b)      | when a/b is cell-type signal, create a pmos/nmos, connect its gate to a/b, source to VDD/VSS, drain to common output. When a/b is single type signal, use it to form PUN/PDN directly. | cost(op) + num(cell-signal)  |
| ! (x)           | connect op to an inverter | 2+cost(op)            |
| +(a,b)          | form a series(PUN)/parralel(PDN) connection between ops, with a being on the top/left. when op is a cell-type signal, create a mosfet, connect op to its gate first. | cost(op) + num(cell-signal) |
| * (a,b)         | form a series(PDN)/parralel(PUN) connection between ops, with a being on the top/left. when op is a cell-type signal, create a mosfet, connect op to its gate first. | cost(op) + num(cell-signal) |
| bridge (a,b,c,d,e)          | form a bridge (left-top, left-bottom(PDN)/right-top(PUN), right-top(PDN)/left-bottom(PUN), right-bottom, middle) connection between ops. when op is a cell-type signal, create a mosfet, connect op to its gate first. | cost(op) + num(cell-signal) |
| &               | virtual output concatenation only | cost(op)              |
| X               | size literal on var signal | 0       |
| Bool            | constant literal (vdd/gnd) | 0                     |
| Var             | input literal | 0                     |

cost(op) = sum of costs of its operands after de-duplication.
num(cell-signal) is the number of direct child operands whose type is cell.* (count each operand once; do not recurse). Size for X(Var, size) contributes to num(cell-signal) by adding size (not 1).

### Deduplication strtegy
E-graph based netlist is hard to tell the concept of instance, which means all the instances of the same logic are equivlant (in E-graph, they share the same enode).
This leads to a fan-out problem, for all of the input of the same logic, we can't tell which instance's output it comes from.
Two naive ways to solve this, is we either consider every cell with the same logic from a same instance, or form an independent instance everytime the logic is used.
We use the first one in the TransLog's implementation.

This calls for a deduplication process during SPICE export and transistor count.
The de-duplication is performed on join and ! signals. Which in circuit, means all join and ! forms a (standarad cell-like) cell, which can have fanouts. For all the places where need the signal of such cell, it checks for duplication and only 1 instance is preserved for cells with same logic.

The detailed de-duplication policy is:
1. the program keeps track of a table using (operator, eclassID, enodeID) as key, where operator={join|!}, which maps to min_cost.
2. when a join/! instance is visited, if the key is new, set min_cost = cost(op); if the key exists, set min_cost = min(min_cost, cost(op)).
3. the cost(op) used by parents is the de-duplicated min_cost from this table.

### Grammar Not closed
**This rewriting system is not closed over the language generated by the grammar.**
Translog’s e-graph equivalence classes are defined by logical equivalence rather than structual equivalence. However, circuit composition must respect functional compatibility, which can yield invalid expressions such as !(a+b); this form has no practical meaning acrooding to semantics defined previously. This also means the grammar plus rewrites is not closed under composition.

To ensure functional correctness of extraction, we treat circuit functionality as an additional equivalence dimension and partition each e-class by function groups. The extraction process can be viewed as recursive circuit construction: starting from an operator or structure, we repeatedly search within its operand e-classes for subcircuits that satisfy the functional requirements of that operator.