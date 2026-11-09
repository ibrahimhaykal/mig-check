# Catatan Belajar Rust: mig-check

Project pribadi buat belajar Rust lewat tool pengecek data antara dua database PostgreSQL.

## Kenapa project ini

Dulu gue pernah memigrasikan sistem dari Oracle ke PostgreSQL dengan dual write di level aplikasi. Risikonya, dua database tidak berada dalam satu transaksi, jadi datanya bisa beda tanpa ketahuan.

`mig-check` menjawab tiga pertanyaan:

1. Datanya sama atau tidak?
2. Bedanya di ID mana?
3. Masalahnya apa?

## Cara menjalankan

Dari terminal Windows:

```bash
docker compose up -d
docker compose exec dev bash
```

Di dalam container:

```bash
cargo run -- --table orders
cargo run --bin simulation_dualwrite
cargo run -- --table sim_orders
cargo run --bin comparation
```

`--` memisahkan argumen untuk Cargo dan argumen untuk program.

## Alur program

1. Hitung jumlah row di source dan target.
2. Ambil `MIN(id)` dan `MAX(id)` dari kedua database.
3. Bagi data menjadi chunk.
4. Bandingkan checksum tiap chunk.
5. Belah chunk yang berbeda sampai tersisa satu ID.
6. Tentukan jenis masalahnya.
7. Tulis hasil ke `report.json`.

## Jenis masalah

| Source | Target | Hasil |
|---:|---:|---|
| 1 row | 0 row | `missing_in_target` |
| 0 row | 1 row | `missing_in_source` |
| 1 row | 1 row, isi beda | `value_mismatch` |

## Hasil pengujian

| Metrik | Hasil |
|---|---:|
| Failure disuntik | 451 |
| Failure ditemukan | 451 |
| Terlewat | 0 |
| False positive | 0 |

Pelajaran utamanya: `COUNT(*)` bisa menunjukkan selisih `0` walaupun ada row yang hilang atau berubah.

## Dasar Rust

### Variabel dan tipe

```rust
let nama = "orders";
let mut antrian = Vec::new();
let jumlah: i64 = 100;
```

- Variabel immutable secara default.
- `mut` membuat variabel bisa berubah.
- `i64` adalah integer 64-bit, cocok dengan `BIGINT` di PostgreSQL.
- `String` adalah teks yang dimiliki variabel.
- `&str` adalah teks yang dipinjam.

### Komentar dan attribute

```rust
// Ini komentar dan diabaikan compiler.

#[derive(Parser)]
struct Args {
    #[arg(long)]
    table: String,
}
```

- `//` adalah komentar.
- `#[...]` adalah attribute dan memengaruhi kode.
- `arg` berasal dari `clap`, bukan bawaan Rust.
- Di dalam string, `//` hanya teks biasa.

### Fungsi

```rust
fn tambah(a: i64, b: i64) -> i64 {
    a + b
}
```

- Tipe parameter wajib ditulis.
- `->` menentukan tipe return.
- Ekspresi terakhir tanpa `;` menjadi nilai return.

### Ownership dan borrowing

- Setiap nilai punya satu pemilik.
- Saat nilai dipindahkan, variabel lama tidak bisa dipakai lagi.
- `&` meminjam nilai tanpa mengambil kepemilikannya.

```rust
count_rows(&source_pool, &args.table).await?;
```

### Option dan Result

```rust
Option<String> // ada String atau None
Result<i64>    // sukses berisi i64 atau error
```

- `Option` menggantikan nilai null.
- `Result` membuat kemungkinan gagal terlihat dari tipe.
- `?` meneruskan error ke pemanggil.

### Struct, enum, dan tuple

```rust
struct Issue {
    id: i64,
    kind: IssueKind,
}

enum IssueKind {
    MissingInTarget,
    MissingInSource,
    ValueMismatch,
}

let hasil: (i64, Option<String>) = (1, None);
let jumlah = hasil.0;
```

- `struct` mengelompokkan data.
- `enum` membatasi nilai ke pilihan yang valid.
- Tuple diakses dengan `.0`, `.1`, dan seterusnya.

### Perulangan dan pattern

```rust
for start in (1..=100).step_by(10) {}

while let Some((start, end)) = antrian.pop() {}

let (Some(min), Some(max)) = (min_id, max_id) else {
    return Ok(());
};
```

- `while let` berjalan selama polanya cocok.
- `let ... else` mengambil nilai jika cocok, atau keluar jika tidak.

### Collection dan closure

- `Vec` adalah list yang bisa bertambah.
- `HashSet` menyimpan nilai unik, mirip `Set` di JavaScript.
- `|c| ...` adalah closure, mirip arrow function.
- `.all(...)` mirip `.every()` di JavaScript.

## Async

```rust
let (source, target) = tokio::try_join!(
    count_rows(&source_pool, &table),
    count_rows(&target_pool, &table)
)?;
```

- `async fn` dijalankan dengan `.await`.
- Tokio adalah runtime untuk menjalankan kode async.
- `try_join!` menjalankan beberapa operasi bersamaan, mirip `Promise.all`.

## Library

| Library | Kegunaan |
|---|---|
| `clap` | Membaca argumen CLI |
| `tokio` | Runtime async |
| `sqlx` | Koneksi, query, bind, mapping hasil |
| `anyhow` | Penanganan error yang praktis |
| `serde` | Serialisasi dan deserialisasi data |
| `serde_json` | Membaca dan menulis JSON |

## SQL dengan sqlx

```rust
let query = format!("SELECT COUNT(*) FROM {table}");

let (count,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(query))
    .fetch_one(pool)
    .await?;
```

- Nilai sebaiknya memakai `$1`, `$2`, dan `.bind()`.
- Nama tabel tidak bisa dikirim sebagai parameter biasa.
- `AssertSqlSafe` menandai SQL dinamis yang sudah diaudit.
- `AssertSqlSafe` tidak membuat query aman. Pengamannya tetap validasi input.

## Istilah database

- **Dual write:** satu operasi aplikasi menulis ke dua database.
- **Checksum:** sidik jari data.
- **Range:** rentang ID yang dicek.
- **Chunk:** potongan data, misalnya per 10.000 ID.
- **Data drift:** data yang seharusnya sama menjadi berbeda.
- **False positive:** tool melaporkan masalah yang sebenarnya tidak ada.
- **Exit code:** angka status saat program selesai; `0` berarti sukses.

## Yang sempat bikin gue bingung

- **Tujuan mig-check:** membuktikan isi dua database sama, bukan cuma jumlah row-nya.
- **Kenapa dipotong per chunk:** supaya tahu area yang berbeda, lalu bisa dipersempit sampai ID persis.
- **`table` bukan database ketiga:** source dan target tetap dua. Nama tabel hanya input untuk tiap pengecekan.
- **Signature fungsi:** `count_rows(pool, table)` harus dipanggil dengan dua argumen. Jangan mengubah `main` menjadi `count_rows`.
- **Tanpa `#[arg(long)]`:** field menjadi argumen posisi, bukan flag `--table`.
- **`Vec<(i64, i64)>`:** tipe ditulis setelah `:`, bukan setelah `=`.
- **`Ok((count, hash))`:** kurung luar milik `Ok`, kurung dalam adalah tuple.
- **Kode belum berubah:** kalau warning lama masih muncul, cek file sudah tersimpan.
- **Berhenti saat sisa dua ID itu salah:** range harus dibelah sampai `start == end`, kalau tidak hasilnya false positive.
- **`docker` di dalam container:** tidak tersedia. Command Docker dijalankan dari terminal Windows.
- **rust-analyzer:** harus jalan di container, karena Rust gue ada di sana.
- **Host database:** dari container pakai `source:5432` dan `target:5432`, bukan `localhost`.
- **`report.json`:** supaya hasilnya bisa disimpan dan dipakai sistem lain.
- **Resume:** seperti save game, supaya proses yang mati bisa lanjut dari chunk terakhir.

## Error yang pernah ditemui

| Error | Penyebab | Solusi |
|---|---|---|
| `unexpected argument '--table'` | Field belum memakai `#[arg(long)]` | Tambahkan attribute |
| `comparison operators cannot be chained` | Menulis `= Vec<...>` | Gunakan `: Vec<...>` |
| `mismatched types` | Tipe argumen tidak sesuai | Samakan dengan signature fungsi |
| `dynamic SQL strings should be audited` | SQL dinamis di `sqlx` 0.9 | Validasi input, lalu pakai `AssertSqlSafe` |
| `docker: command not found` | Menjalankan Docker di dalam container | Gunakan terminal Windows |
| `spawn UNKNOWN` di rust-analyzer | Extension versi Windows gagal berjalan | Install di dalam container |

## Batasan saat ini

- Hanya PostgreSQL ke PostgreSQL.
- Tabel harus punya kolom numerik bernama `id`.
- Belum aman untuk production.
- Data yang sedang ditulis bisa menghasilkan false positive.

## Berikutnya

- [ ] Resume setelah proses mati
- [ ] Throttle
- [ ] Exit code
- [ ] Pengecekan ulang drift
- [ ] Primary key non-numerik dan composite
- [ ] Dukungan MySQL atau Oracle ke PostgreSQL