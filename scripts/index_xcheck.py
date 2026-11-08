#!/usr/bin/env python3
"""Valida el SQL del índice contra SQLite real: ejecuta las sentencias generadas por `sqlgen` (Rust) y compara con MemStore.

Pasos: cargo run --example index_xcheck -- <dir>  ·  python scripts/index_xcheck.py <dir> [--bench]
Comprueba: esquema, upsert sin duplicados, FTS5 trigram (y la alternativa con instr), 100+ consultas (resultado y orden exactos),
barrido de generaciones, borrado de árboles y persistencia al cerrar/reabrir un archivo de base de datos.
"""
import os, sqlite3, sys, tempfile, time

d = sys.argv[1]
bench = "--bench" in sys.argv
rd = lambda n: open(os.path.join(d, n), encoding="utf8").read()
unesc = lambda s: s.replace("\\t", "\t").replace("\\n", "\n").replace("\\\\", "\\")
print("SQLite", sqlite3.sqlite_version)

path = os.path.join(tempfile.mkdtemp(), "index.db")
db = sqlite3.connect(path)
db.execute("PRAGMA journal_mode=WAL"); db.execute("PRAGMA synchronous=NORMAL")
db.executescript(rd("schema.sql"))
try:
    db.executescript(rd("fts.sql")); has_fts = True
except sqlite3.OperationalError as e:
    has_fts = False; print("FTS5 trigram no disponible:", e)
print("FTS5 trigram:", has_fts)

rows = []
for line in rd("entries.tsv").splitlines():
    f = line.split("\t")
    n = lambda v: None if v == "\\N" else int(v)
    rows.append((int(f[0]), unesc(f[1]), unesc(f[2]), unesc(f[3]), unesc(f[4]), f[5], int(f[6]), int(f[7]), n(f[8]), n(f[9]), n(f[10]), int(f[11]), int(f[12])))
upsert = rd("upsert.sql")
t = time.time()
with db:
    db.executemany(upsert, rows)
print(f"upsert {len(rows)} filas: {time.time()-t:.2f}s")
count = lambda: db.execute("select count(*) from entries").fetchone()[0]
assert count() == len(rows)
with db:  # reescritura idéntica: nada se duplica
    db.executemany(upsert, rows)
assert count() == len(rows), "el upsert duplicó filas"
if has_fts:
    assert db.execute("select count(*) from entries_fts").fetchone()[0] == len(rows), "FTS desincronizado tras upsert repetido"

ok = bad = 0
lines = rd("queries.tsv").splitlines()
for line in lines:
    i, fts, sql, params, more, expected = line.split("\t")
    if fts == "1" and not has_fts:
        continue
    ps = [int(p[2:]) if p[0] == "I" else unesc(p[2:]) for p in params.split("\x1f")] if params else []
    got = db.execute(unesc(sql), ps).fetchall()
    limit = ps[-2] - 1
    page = [r[0] for r in got[:limit]]
    exp = [unesc(x) for x in expected.split("\x1f")] if expected else []
    if page == exp and int(more) == int(len(got) > limit):
        ok += 1
    else:
        bad += 1
        print(f"DIFERENCIA consulta #{i} fts={fts}: sqlite={len(page)} memstore={len(exp)} more={len(got)>limit}/{more}")
        for a, b in zip(page, exp):
            if a != b: print("   primera diferencia:", a, "|", b); break
        print("   SQL:", unesc(sql)[:300]); print("   params:", ps)
print(f"consultas: {ok} coinciden, {bad} difieren")
assert bad == 0 and ok > 0

# Barrido por generaciones: tras un indexado completo (gen=2) que solo ve la mitad de la raíz 1, lo demás desaparece.
sweep, rm = rd("sweep.sql"), rd("remove_tree.sql")
r1 = [r for r in rows if r[0] == 1]; seen = r1[: len(r1) // 2]
with db:
    db.executemany(upsert, [r[:12] + (2,) for r in seen])
    removed = db.execute(sweep, (1, 2)).rowcount
assert removed == len(r1) - len(seen), (removed, len(r1) - len(seen))
assert db.execute("select count(*) from entries where root_id=1").fetchone()[0] == len(seen)
assert count() == len(rows) - removed
if has_fts:
    assert db.execute("select count(*) from entries_fts").fetchone()[0] == count(), "FTS desincronizado tras el barrido"
    assert db.execute("select count(*) from entries_fts where entries_fts match '\"foto\"'").fetchone()[0] == db.execute("select count(*) from entries where instr(name_lc,'foto')>0").fetchone()[0]
# Árbol: la carpeta y sus descendientes, pero no una hermana con prefijo de texto igual.
pre = "d:/datos2"
exp_rm = len([r for r in rows if r[1] == pre or r[1].startswith(pre + "/")])
before = count()
with db:
    n = db.execute(rm, (pre, pre + "/", pre + "0")).rowcount
assert n == exp_rm and count() == before - n, (n, exp_rm)
# Persistencia: cerrar y reabrir el archivo.
total = count(); db.close()
db2 = sqlite3.connect(path)
assert db2.execute("select count(*) from entries").fetchone()[0] == total
assert db2.execute("select count(*) from entries_fts where entries_fts match '\"foto\"'").fetchone()[0] > 0 if has_fts else True
assert db2.execute("pragma integrity_check").fetchone()[0] == "ok"
print("barrido, borrado de árboles y persistencia tras reabrir: OK")
print("TODO OK")
