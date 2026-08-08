# EpochQL Language Reference (v1.0)

EpochQL is a context-aware query and mutation language for **epochs** Merkle-DAG
state. It combines Cypher-style graph pattern matching with git-like version
control primitives.

The parser is a hand-written lexer + recursive-descent parser in the `epochql`
crate (no `nom` / `pest`). Execution against [`DiskStore`] is a follow-up phase.

## Quick examples

```sql
-- Time travel into a commit, match state nodes
USE COMMIT "e8a31f" MATCH (n:State) SELECT n.payload;

-- Multi-hypothesis agent workflow
CREATE BRANCH plan_alpha FROM HEAD;
CREATE BRANCH plan_beta FROM HEAD;
USE BRANCH plan_alpha;
COMMIT { action: "API_CALL", status: "success" } MESSAGE "Executed plan alpha";
DIFF BRANCH plan_alpha AND BRANCH plan_beta;
MERGE BRANCH plan_alpha INTO BRANCH main STRATEGY FAST_FORWARD;
```

## Statements

Every statement is optionally terminated by `;`. Scripts may contain multiple
statements.

| Kind | Keywords | Purpose |
|------|----------|---------|
| Query | `USE`, `MATCH`, `TRAVERSE`, `WHERE`, `SELECT` | Read / analyze DAG state |
| Version | `COMMIT`, `CREATE`/`DELETE BRANCH`, `CHECKOUT`, `MERGE`, `DIFF` | Mutate DAG / refs |

### Context (`USE`)

Anchors subsequent query clauses to a point in history:

```
USE HEAD
USE BRANCH main
USE BRANCH "hypothesis_b"
USE COMMIT "abc123"          -- full or hex prefix
USE TAG release_1            -- reserved for future tags
```

### Pattern matching (`MATCH`)

```
MATCH (a:Commit)-[:CHILD]->(b)
MATCH (child)<-[:PARENT*1..10]-(parent:Commit)
MATCH (c:Commit {author: "agent_1"})
```

**Bare arrows**

| Syntax | Direction | Default edge type |
|--------|-----------|-------------------|
| `->` | outgoing | `CHILD` |
| `<-` | incoming | `PARENT` |

**Typed edges:** `PARENT`, `CHILD`, `ANCESTOR`, `DESCENDANT`

**Hop multipliers:** `*`, `*N`, `*N..M`, `*N..`, `*..M`

### Traversal (`TRAVERSE`)

```
TRAVERSE MERGE_BASE(BRANCH a, BRANCH b)
TRAVERSE SHORTEST_PATH(COMMIT "aa", COMMIT "bb")
TRAVERSE ANCESTORS(HEAD)
```

### Filters & projection

```
WHERE child.hash = "f7b10a" AND child.author != "bot"
SELECT parent.hash, parent.timestamp AS ts
```

Operators: `=`, `!=`, `AND`, `OR`. Property access via `.`.

### Mutations

```
COMMIT { key: "value", n: 1 } MESSAGE "msg" PARENTS [HEAD, BRANCH other]
CREATE BRANCH name FROM HEAD
DELETE BRANCH name
CHECKOUT BRANCH main
MERGE BRANCH src INTO BRANCH dst STRATEGY FAST_FORWARD | THREE_WAY | SQUASH
DIFF BRANCH a AND BRANCH b PATH "agent.context"
```

## Lexical notes

- Keywords are **case-insensitive**
- Keywords may also be used as **identifiers** when a name is expected (e.g. variable `child`, label `Commit`) — original spelling is preserved
- Identifiers: `[a-zA-Z_][a-zA-Z0-9_]*`
- Strings: `"..."` with escapes `\n \t \r \" \\`
- Line comments: `-- ...`
- Commit hashes: hex strings, 1–64 characters (prefix resolution at execute time)
- Branch names: bare identifier or `"quoted string"`

## Rust API

```rust
use epochql::{parse, parse_script};

let stmt = parse(r#"CHECKOUT HEAD"#)?;
let script = parse_script(r#"
    CREATE BRANCH x FROM HEAD;
    USE BRANCH x;
"#)?;
```

## Status

| Layer | Status |
|-------|--------|
| Lexer | Done |
| Parser → AST | Done |
| Schemaless executor | Done |
| `.eql` migrations / path indexes | Format defined; runner next |
| CLI `epochs query` | Executes against `DiskStore` |
