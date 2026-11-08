#!/usr/bin/env python3
"""Mide con SQLite real el SQL de producción del índice sobre un conjunto de datos controlado y sintético.
Uso: python scripts/index_bench.py <dir con salida de index_xcheck> [filas=1000000]
Mide: ingesta (upsert + disparadores FTS), consultas representativas (mín. y mediana de 7 ejecuciones, caché caliente), tamaño en disco.
Las cifras dependen de la máquina y NO son cifras de Windows: repite en el equipo de destino antes de citarlas."""
import os, random, sqlite3, statistics, sys, tempfile, time

d = sys.argv[1]; N = int(sys.argv[2]) if len(sys.argv) > 2 else 1_000_000
rd = lambda n: open(os.path.join(d, n), encoding="utf8").read()
unesc = lambda s: s.replace("\\t", "\t").replace("\\n", "\n").replace("\\\\", "\\")
random.seed(11)
W = ["informe","foto","vacaciones","proyecto","factura","musica","video","curso","tesis","backup","diseno","logo","cliente","notas","final","borrador","scan","reporte","datos"]
E = ["pdf","png","jpg","psd","docx","xlsx","mp4","txt","zip","rs","ts","dll","","mp3"]
path = os.path.join(tempfile.mkdtemp(), "bench.db")
db = sqlite3.connect(path)
db.execute("PRAGMA journal_mode=WAL"); db.execute("PRAGMA synchronous=NORMAL"); db.execute("PRAGMA cache_size=-65536")
db.executescript(rd("schema.sql")); db.executescript(rd("fts.sql"))
up = rd("upsert.sql"); roots = ["c:", "d:", "e:"]
def gen(n):
    for i in range(n):
        ext = random.choice(E); isdir = random.random() < 0.1
        if isdir: ext = ""
        base = "_".join(random.choices(W, k=random.randint(1, 3))) + str(random.randint(0, 9999))
        name = base + ("." + ext if ext else "")
        r = random.randrange(3); sub = "/".join(random.choice(W) + str(random.randrange(30)) for _ in range(random.randint(0, 4)))
        key = f"{roots[r]}/{sub + '/' if sub else ''}{i}_{name}".lower()
        size = 0 if isdir else (random.randint(10**9, 5*10**9) if random.random() < .02 else random.randint(0, 5*10**6))
        yield (r + 1, key, key, name, name.lower(), ext, 1 if isdir else 0, size, 1_600_000_000_000 + random.randint(0, 2*10**11), None, None, key.count("/"), 1)
db.execute("insert into roots(path) values('c:'),('d:'),('e:')")
t = time.time(); done = 0
while done < N:
    chunk = list(gen(min(2000, N - done)))
    for j, row in enumerate(chunk): chunk[j] = (row[0], row[1].replace(f"/{done}_", f"/{done+j}_", 1) if False else row[1] + f"#{done+j}") + row[2:]
    with db: db.executemany(up, chunk)
    done += len(chunk)
dt = time.time() - t
db.execute("insert into entries_fts(entries_fts) values('optimize')"); db.commit()
print(f"ingesta {N:,} filas con FTS: {dt:.1f}s ({N/dt:,.0f} filas/s) · archivo {os.path.getsize(path)/2**20:.0f} MB (+WAL {os.path.getsize(path+'-wal')/2**20 if os.path.exists(path+'-wal') else 0:.0f} MB)")
db.execute("PRAGMA wal_checkpoint(TRUNCATE)")
db.execute("ANALYZE"); db.commit()
print(f"archivo tras checkpoint: {os.path.getsize(path)/2**20:.0f} MB")
rows = [l.split("\t") for l in rd("queries.tsv").splitlines() if l.split("\t")[1] == "1"]
# Consultas representativas, elegidas por número de la batería de `index_xcheck.rs` (columna 1 de queries.tsv).
CASES = [
    (0, "«foto» (nombre común, ordena por relevancia)"), (10, "«foto vacaciones» (dos términos)"), (11, "«ab foto» (corto + FTS)"),
    (2, "«fo» (2 caracteres, sin FTS)"), (3, "«a» (1 carácter)"), (25, "exacta «informe.pdf»"), (28, "extensión PSD"),
    (51, "PSD de más de 1 GB"), (56, "«info» + rango de fechas"), (63, "carpeta c:/ (subárbol)"),
    (82, "«o» página 0 (60 resultados)"), (85, "«o» página con offset 5000"), (86, "«e» hasta 5000 resultados"), (87, "sin texto: lo más reciente"),
]
byid = {int(r[0]): r for r in rows}
print(f"{'consulta':48s} {'filas':>6s} {'mín ms':>8s} {'mediana ms':>11s}")
for qid, label in CASES:
    r = byid[qid]; sql = unesc(r[2]); ps = [int(p[2:]) if p[0] == "I" else unesc(p[2:]) for p in r[3].split("\x1f")] if r[3] else []
    ts = []
    for _ in range(7):
        t = time.perf_counter(); res = db.execute(sql, ps).fetchall(); ts.append((time.perf_counter() - t) * 1000)
    print(f"{label:48s} {len(res):6d} {min(ts):8.1f} {statistics.median(ts):11.1f}")
