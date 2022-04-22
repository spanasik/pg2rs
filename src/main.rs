use clap::{Arg, command};
use convert_case::{Case, Casing};
use futures::future;
use inflection::{singular};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs::File;
use std::io::{Write as IoWrite};
use std::sync::{Arc};
use tokio_postgres::{NoTls, Error};

extern crate pretty_env_logger;
#[macro_use] extern crate log;

#[derive(Debug)]
struct ColumnProperties {
    name: String,
    rust_type: String
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::try_init_custom_env("LOG_LEVEL").unwrap();

    let matches = command!()
        .arg(Arg::new("connection-string")
            .required_unless_present_all(
                &["user", "password", "host", "port", "database"]
            )
            .short('c')
            .long("connection-string")
            .takes_value(true)
            .env("POSTGRES_CONNECTION_STRING")
            .help("full connection string instead of separate credentials \
              in a form postgresql://username:password@host:port/dbname"))
        .arg(Arg::new("user")
            .long("user")
            .short('u')
            .required_unless_present("connection-string")
            .conflicts_with("connection-string")
            .takes_value(true)
            .env("POSTGRES_USER"))
        .arg(Arg::new("password")
            .long("password")
            .short('p')
            .required_unless_present("connection-string")
            .conflicts_with("connection-string")
            .takes_value(true)
            .env("POSTGRES_PASSWORD"))
        .arg(Arg::new("host")
            .long("host")
            .short('h')
            .required_unless_present("connection-string")
            .conflicts_with("connection-string")
            .takes_value(true)
            .env("POSTGRES_HOST"))
        .arg(Arg::new("port")
            .long("port")
            .short('r')
            .required_unless_present("connection-string")
            .conflicts_with("connection-string")
            .takes_value(true)
            .validator(|s| s.parse::<usize>())
            .env("POSTGRES_PORT"))
        .arg(Arg::new("database")
            .long("database")
            .short('d')
            .required_unless_present("connection-string")
            .conflicts_with("connection-string")
            .takes_value(true)
            .env("POSTGRES_DATABASE"))
        .arg(Arg::new("schema")
            .long("schema")
            .short('s')
            .takes_value(true)
            .required(true)
            .env("POSTGRES_SCHEMA"))
        .arg(Arg::new("table")
            .long("table")
            .short('t')
            .takes_value(true)
            .env("POSTGRES_TABLE"))
        .arg(Arg::new("postgres_crate")
            .long("postgres_crate")
            .short('w')
            .takes_value(true)
            .default_value("postgres")
            .possible_values(&["postgres", "tokio_postgres"])
            .env("POSTGRES_CRATE")
            .help("Postgres crate"))
        .arg(Arg::new("singularize-table-names")
            .long("singularize-table-names")
            .short('z')
            .required(false)
            .takes_value(false)
            .env("SINGULARIZE_TABLE_NAMES"))
        .arg(Arg::new("use-chrono-crate")
            .long("use-chrono-crate")
            .short('n')
            .required(false)
            .takes_value(false)
            .env("USE_CHRONO_CRATE")
            .help("use chrono DateTime for timestamps"))
        .arg(Arg::new("use-rust-decimal")
            .long("use-rust-decimal")
            .short('m')
            .required(false)
            .takes_value(false)
            .env("USE_RUST_DECIMAL")
            .help("use chrono DateTime for timestamps"))
        .arg(Arg::new("output_file")
            .long("output_file")
            .short('o')
            .takes_value(true)
            .env("OUTPUT_FILE")
            .help("output file path"))
        .get_matches();

    let connection_string = match matches.value_of("connection-string") {
        Some(s) => String::from(s),
        None => {
            format!(
                "postgresql://{}:{}@{}:{}/{}",
                matches.value_of("user").unwrap(),
                matches.value_of("password").unwrap(),
                matches.value_of("host").unwrap(),
                matches.value_of("port").unwrap(),
                matches.value_of("database").unwrap(),
            )
        }
    };
    debug!("Using connection string: {}", connection_string);

    let schema = matches.value_of("schema").unwrap();
    debug!("Using schema: {}", schema);

    let postgres_crate = matches.value_of("postgres_crate").unwrap();
    debug!("Using Postgres crate: {}", postgres_crate);

    let singularize_table_names = matches.is_present("singularize-table-names");
    debug!("Singularize table names: {}", singularize_table_names);

    let use_chrono_crate = matches.is_present("use-chrono-crate");
    debug!("Use chrono crate: {}", use_chrono_crate);
    let timestamp_type =
      if use_chrono_crate { "DateTime<Utc>" } else { "String" };

    let use_rust_decimal = matches.is_present("use-rust-decimal");
    debug!("Use rust-decimal: {}", use_rust_decimal);
    let numeric_type =
      if use_rust_decimal { "Decimal" } else { "String" };

    let output_file = match matches.value_of("output_file") {
        Some(value) => { value }
        None => ""
    };
    debug!("Output file: \"{}\"", output_file);

    // Connect to the database.
    let (client, connection) =
        tokio_postgres::connect(&connection_string, NoTls).await.unwrap();
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });
    debug!("Connected to database");

    let tables_list: Vec<String> = match matches.value_of("table") {
        Some(s) => vec![String::from(s)], // assume user awareness about table presence
        None => {
            debug!("List tables in schema '{}'", schema);
            client.query(
                "SELECT a.relname AS name FROM pg_class a
                    LEFT OUTER JOIN pg_description b ON b.objsubid = 0 AND a.oid = b.objoid
                    WHERE a.relnamespace = (
                    SELECT oid FROM pg_namespace WHERE nspname = $1
                    ) AND a.relkind = 'r' ORDER BY a.relname;", &[&schema]
            ).await.unwrap().iter().map( | row | {
                row.get(0)
            }).collect()
        }
    };
    if tables_list.is_empty() {
        println!("No tables found in specified schema");
        return Ok(());
    }

    let client_arc = Arc::new(client);
    debug!("Tables: {:?}", tables_list);
    let tables_data: BTreeMap<String, Vec<ColumnProperties>> = BTreeMap::from_iter(
      future::join_all(tables_list.iter().map(| table_name | {
        let client_clone = client_arc.clone();
        async move {
        debug!("List columns for table '{}'", table_name);
        let columns_data: Vec<ColumnProperties> = client_clone.query(
            "SELECT column_name, udt_name, is_nullable
             FROM information_schema.columns
             WHERE table_schema = $1 AND table_name = $2
             ORDER BY ordinal_position;",
             &[&schema, &table_name]
        ).await.unwrap().iter().map( | row | {
            let is_nullable = row.get(2);
            ColumnProperties {
                name: row.get(0),
                rust_type: match row.get(1) {
                    "bytea" => type_str(is_nullable, "Vec<u8>"),
                    "text" => type_str(is_nullable, "String"),
                    "varchar"|"character varying"|"bpchar" => type_str(is_nullable, "String"),
                    "char"|"character" => type_str(is_nullable, "i8"),
                    "smallint"|"int2"|"smallserial"|"serial2" => type_str(is_nullable, "i16"),
                    "integer"|"int"|"int4"|"serial"|"serial4" => type_str(is_nullable, "i32"),
                    "bigint"|"int8"|"bigserial"|"serial8" => type_str(is_nullable, "i64"),
                    "oid" => type_str(is_nullable, "u32"),
                    "real"|"float4" => type_str(is_nullable, "f32"),
                    "double precision"|"float8" => type_str(is_nullable, "f64"),
                    "bool"|"boolean" => type_str(is_nullable, "bool"),
                    "numeric"|"decimal" => type_str(is_nullable, numeric_type),
                    "timestamp"|"timestamptz" => type_str(is_nullable, timestamp_type),
                    _ => type_str_transform_case(
                        is_nullable, row.get(1), Case::UpperCamel) // enums etc
                }
            }
        }).collect();
        let mut result_table_name: String = table_name.to_string();
        if  singularize_table_names {
            result_table_name = singular::<_, String>(table_name);
            debug!("singularized table name: {}", table_name);
        }
        (result_table_name, columns_data)
      }
      })).await.into_iter());

    debug!("tables_data: {:#?}", tables_data);

    let enums_data: BTreeMap<String, Vec<String>> = client_arc.clone().query(
        "SELECT n.nspname AS enum_schema,  
            t.typname AS enum_name,
            string_agg(e.enumlabel, ',') AS enum_value
            FROM pg_type t 
            JOIN pg_enum e ON t.oid = e.enumtypid  
            JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = $1
            GROUP BY enum_schema, enum_name;", &[&schema]
    ).await.unwrap().iter().map( | row | {
        (row.get(1), row.get::<_, &str>(2).split(',').map( | i | { String::from(i) }).collect())
    }).collect();
    debug!("Enums: {:?}", enums_data);

    let mut output = String::new();
    writeln!(output, "// autogenerated using pg2rs").unwrap();

    if !enums_data.is_empty() {
        writeln!(output, "use std::str::FromStr;").unwrap();
    }

    writeln!(output, "use {}::row::Row;", postgres_crate).unwrap();
    writeln!(output, "use {}::types::{{ToSql, FromSql}};", postgres_crate).unwrap();

    if use_chrono_crate {
        writeln!(output).unwrap();
        writeln!(output, "extern crate chrono;").unwrap();
        writeln!(output, "use chrono::{{DateTime, Utc}};").unwrap();
    }

    if use_rust_decimal {
        writeln!(output).unwrap();
        writeln!(output, "use rust_decimal::Decimal;").unwrap();
    }
    
    process_enums(&enums_data, &mut output);
    process_tables_data(&tables_data, &mut output);

    if output_file.is_empty() {
        print!("{}", output);
    } else {
        let mut fp = File::create(output_file).unwrap();
        write!(fp, "{}", output).unwrap();
    }
    Ok(())
}

fn type_str<'a>(nullable: &'a str, type_name: &'a str) -> String {
    match nullable {
        "YES" => format!("Option<{}>", type_name),
        "NO" => type_name.to_string(),
        _ => type_name.to_string()
    }
}

fn type_str_transform_case<'a>(nullable: &'a str, type_name: &'a str, case: Case) -> String {
    let type_name = type_name.to_case(case);
    match nullable {
        "YES" => format!("Option<{}>", type_name),
        "NO" => type_name,
        _ => type_name
    }
}

fn process_enums(enums_data: &BTreeMap<String, Vec<String>>, output: &mut String) {
    for (enum_name, variants) in enums_data {
        writeln!(output).unwrap();
        writeln!(output, "#[derive(Debug, ToSql, FromSql)]").unwrap();
        writeln!(output, "#[postgres(name = \"{}\")]", enum_name).unwrap();
        let enum_name = enum_name.to_case(Case::UpperCamel);
        writeln!(output, "pub enum {} {{", enum_name).unwrap();
        for variant in variants {
            writeln!(output, "#[postgres(name = \"{}\")]", variant).unwrap();
            writeln!(output, "    {},", variant.to_case(Case::UpperCamel)).unwrap();
        }
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "impl FromStr for {} {{
    type Err = ();
    fn from_str(input: &str) -> Result<{}, Self::Err> {{
        match input {{", enum_name, enum_name).unwrap();
        for variant in variants {
            writeln!(
                output, "            \"{}\"  => Ok({}::{}),",
                variant, enum_name, variant.to_case(Case::UpperCamel)).unwrap();
        }
        writeln!(output, "            _      => Err(()),
        }}
    }}
}}").unwrap();
    }
}

fn process_tables_data(tables_data: &BTreeMap<String, Vec<ColumnProperties>>, output: &mut String) {
    for (table_name, columns_properties) in tables_data {
        writeln!(output).unwrap();
        writeln!(output, "#[derive(Debug, ToSql, FromSql)]").unwrap();
        writeln!(output, "pub struct {} {{", table_name).unwrap();
        for column in columns_properties {
            writeln!(output,
                "    pub {}: {},",
                column.name.to_case(Case::Snake), column.rust_type
            ).unwrap();
        }
        writeln!(output, "}}").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "impl From<Row> for {} {{", table_name).unwrap();
        writeln!(output, "    fn from(row: Row) -> Self {{").unwrap();
        writeln!(output, "        Self {{").unwrap();
        for column in columns_properties {
            writeln!(output,
                "            {}: row.get(\"{}\"),",
                column.name.to_case(Case::Snake), column.name
            ).unwrap();
        }
        writeln!(output, "        }}").unwrap();
        writeln!(output, "    }}").unwrap();
        writeln!(output, "}}").unwrap();
    }
}