# pg2rs

Command-line tool to make Rust source code entities from Postgres tables.
Generates: 
 - enums
 - structs

which can be then used like

```
mod structs;
use structs::{User};

let result = client.query("SELECT * FROM \"Users\"", &[]).await.unwrap();
let users: Vec<User> = result.into_iter().map(|row| User::from(row)).collect();
println!("{:#?}", users);
```

It is not an ORM, main purpose is to have tables DDL as a source of truth. DDL can be generated using some tool like [Alembic](https://alembic.sqlalchemy.org/en/latest/), and then pg2rs is used to reflect changes in your `structs.rs`


## Usage

```shell
$ pg2rs --help
pg2rs 0.0.2
Stanislav Panasik <spanasik@gmail.com>
Make Rust entities from PostgreSQL schema

USAGE:
    pg2rs [OPTIONS] --schema <schema>

OPTIONS:
    -c, --connection-string <connection-string>
            full connection string instead of separate credentials in a form
            postgresql://username:password@host:port/dbname [env: POSTGRES_CONNECTION_STRING=]

    -d, --database <database>
            [env: POSTGRES_DATABASE=]

    -h, --host <host>
            [env: POSTGRES_HOST=]

        --help
            Print help information

    -m, --use-rust-decimal
            use chrono DateTime for timestamps [env: USE_RUST_DECIMAL=]

    -n, --use-chrono-crate
            use chrono DateTime for timestamps [env: USE_CHRONO_CRATE=]

    -o, --output_file <output_file>
            output file path [env: OUTPUT_FILE=]

    -p, --password <password>
            [env: POSTGRES_PASSWORD=]

    -r, --port <port>
            [env: POSTGRES_PORT=]

    -s, --schema <schema>
            [env: POSTGRES_SCHEMA=]

    -t, --table <table>
            [env: POSTGRES_TABLE=]

    -u, --user <user>
            [env: POSTGRES_USER=]

    -V, --version
            Print version information

    -w, --postgres_crate <postgres_crate>
            Postgres crate [env: POSTGRES_CRATE=] [default: postgres] [possible values: postgres,
            sqlx, tokio_postgres]

    -z, --singularize-table-names
            [env: SINGULARIZE_TABLE_NAMES=]
```


## Roadmap
- Add unit tests

## Authors
- [Stanislav Panasik](https://www.github.com/spanasik)

## License
[MIT](https://choosealicense.com/licenses/mit/)

