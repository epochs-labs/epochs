# Epoch Protocol (EPX) v1

Native wire protocol for epochs — shaped for **Merkle-DAG / content-addressed**
ops, not SQL row CRUD.

Postgres/MySQL speak custom binary protocols on TCP. EPX does the same idea,
but with verbs that match commits, refs, and CAS objects.

## Connection

| | |
|--|--|
| URL shape | `epochs://host:7420` |
| Default port | **7420** |
| Framing | `u32` BE length + UTF-8 JSON body |
| Max frame | 16 MiB |

## Request / response

```json
{"id": 1, "method": "hello", "params": {}}
{"id": 1, "ok": true, "result": { "protocol": "epx", "version": 1, ... }}

{"id": 2, "method": "query", "params": { "sql": "MATCH (c:Commit) SELECT c.hash;" }}
{"id": 2, "ok": false, "error": "..."}
```

## Methods

| Method | Params | Result |
|--------|--------|--------|
| `hello` | `{}` | protocol/version/methods |
| `query` | `{ "sql": "<EpochQL>" }` | `{ "results": [ ... ] }` |
| `refs` | `{}` | `{ "branches": [{ "name", "tip" }] }` |
| `get` | `{ "hash": "<hex>" }` | CAS object `{ hash, type, payload_base64, len }` |

`get` is the content-addressed path — fetch any stored object by hash (commit /
HAMT node / leaf). That has no clean analogue in a generic SQL wire protocol.

## Why not reuse Postgres wire / MySQL?

Those protocols assume tables, rows, prepared statements, and transactional SQL.
epochs is commit DAG + HAMT + EpochQL. EPX stays small and explicit so sync /
pack / watch can be added as new methods without pretending to be SQL.

## Example (conceptual)

```text
connect epochs://127.0.0.1:7420
→ {"id":1,"method":"hello","params":{}}
→ {"id":2,"method":"refs","params":{}}
→ {"id":3,"method":"query","params":{"sql":"COMMIT { status: \"ok\" } MESSAGE \"hi\";"}}
→ {"id":4,"method":"get","params":{"hash":"<tip>"}}
```
