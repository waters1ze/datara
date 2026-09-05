# Datara Embedded Standard Library Catalog

Datara includes 33 embedded, zero-dependency standard library modules compiled directly into the Forgen binary.

---

## Standard Modules Overview

| Category | Module Path | Primary Types / APIs |
|---|---|---|
| **Async / Proactor** | `stdlib.async.future` | `Future.ready()`, `pending()`, `failed()`, `join()`, `unwrap()` |
| | `stdlib.async.task` | `Task.spawn()`, `complete()`, `fail()`, `is_done()` |
| | `stdlib.async.event_loop` | `EventLoop.new()`, `schedule()`, `run_until_complete()` |
| **UI & Canvas** | `stdlib.ui.native` | `NativeWindow.create()`, `Canvas`, `draw_rect()`, `render()` |
| | `stdlib.ui.reactive` | `VNode.new()`, `set_text()`, `diff()` |
| **Collections** | `stdlib.collections.list` | `ListWrapper<T>`, `get_head()`, `count()` |
| | `stdlib.collections.map` | `MapWrapper<K, V>`, `get()`, `insert()` |
| **Math** | `stdlib.math.Math` | `sqrt()`, `pow()`, `abs()`, `min()`, `max()`, `sin()`, `cos()` |
| **Result / Option** | `stdlib.result.result` | `Outcome<T>`, `is_ok()`, `unwrap()` |
| | `stdlib.result.option` | `Maybe<T>`, `is_some()`, `unwrap()` |
| **I/O & System** | `stdlib.io.fs` | `File`, `FileSystem`, `read()`, `write()` |
| | `stdlib.io.args` | `Args`, `get()`, `len()` |
| | `stdlib.io.env` | `Env`, `get_var()`, `set_var()` |
| | `stdlib.sys.process` | `Process`, `exec()`, `exit()` |
| **Networking** | `stdlib.http.client` | `HttpClient`, `get()`, `post()` |
| | `stdlib.http.server` | `HttpServer`, `listen()`, `handle()` |
| | `stdlib.net.socket` | `TcpSocket`, `connect()`, `send()`, `recv()` |
| **JSON** | `stdlib.json.parser` | `JsonParser.parse(str) -> JsonValue` |
| | `stdlib.json.serializer` | `JsonSerializer.stringify(val) -> Str` |
| **Crypto** | `stdlib.crypto.hash` | `Sha256`, `Md5`, `digest()`, `hex()` |
| **Database** | `stdlib.database.kv` | `KvStore`, `set()`, `get()` |
| | `stdlib.database.redis` | `RedisClient`, `connect()`, `query()` |
| | `stdlib.database.sql` | `SqlConnection`, `query()`, `execute()` |
| **AI / Tensors** | `stdlib.ai.tensor` | `Tensor`, `zeros()`, `matmul()`, `relu()` |
