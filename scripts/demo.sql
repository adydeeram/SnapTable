-- Sample "shop" database used in screenshots and for trying SnapTable.
CREATE TABLE customers (
  id serial PRIMARY KEY,
  name text NOT NULL,
  email text UNIQUE,
  created_at timestamptz DEFAULT now(),
  vip boolean DEFAULT false,
  meta jsonb
);
CREATE TABLE orders (
  id bigserial PRIMARY KEY,
  customer_id int REFERENCES customers(id),
  total numeric(10, 2),
  status text,
  placed_at timestamptz DEFAULT now()
);
INSERT INTO customers (name, email, vip, meta)
SELECT 'Customer ' || g, 'c' || g || '@example.com', g % 10 = 0,
       jsonb_build_object('tier', g % 3, 'tags', jsonb_build_array('a', 'b'))
FROM generate_series(1, 2500) g;
INSERT INTO orders (customer_id, total, status, placed_at)
SELECT 1 + (g % 2500), round((random() * 500)::numeric, 2),
       (ARRAY['new', 'paid', 'shipped'])[1 + g % 3],
       now() - (g || ' minutes')::interval
FROM generate_series(1, 150000) g;
CREATE VIEW vip_customers AS SELECT * FROM customers WHERE vip;
CREATE SCHEMA analytics;
CREATE TABLE analytics.daily_revenue AS
SELECT placed_at::date AS day, sum(total) AS revenue FROM orders GROUP BY 1;
ALTER TABLE analytics.daily_revenue ADD PRIMARY KEY (day);
