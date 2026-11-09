CREATE TABLE orders (
  id BIGINT PRIMARY KEY,
  customer_id INT NOT NULL,
  amount NUMERIC(12,2) NOT NULL,
  status TEXT NOT NULL,
  updated_at TIMESTAMPTZ NOT NULL
);

INSERT INTO orders
SELECT
  i,
  i % 1000,
  (i * 7 % 10000) / 100.0,
  (ARRAY['pending', 'paid', 'shipped'])[(i % 3) + 1],
  TIMESTAMPTZ '2026-01-01 00:00:00+00' + i * INTERVAL '1 minute'
FROM generate_series(1, 100000) AS i;