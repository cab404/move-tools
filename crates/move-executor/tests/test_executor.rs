use dialects::{DFinanceDialect, Dialect};
use move_executor::compile_and_execute_script;
use shared::errors::ExecCompilerError;
use utils::tests::{existing_file_abspath, get_modules_path, get_stdlib_path};
use utils::{io, leaked_fpath, FilePath};

fn get_record_module_dep() -> (FilePath, String) {
    let text = r"
address 0x111111111111111111111111 {
    module Record {
        use 0x0::Transaction;

        resource struct T {
            age: u8,
            doubled_age: u8
        }

        public fun create(age: u8): T {
            T { age, doubled_age: age * 2 }
        }

        public fun save(record: T) {
            move_to_sender<T>(record);
        }

        public fun with_incremented_age(): T acquires T {
            let record: T;
            record = move_from<T>(Transaction::sender());
            record.age = record.age + 1;
            record
        }
    }
}
        "
    .to_string();
    let fpath = leaked_fpath(get_modules_path().join("record.move"));
    (fpath, text)
}

fn get_sender() -> String {
    "0x111111111111111111111111".to_string()
}

fn get_script_path() -> FilePath {
    leaked_fpath(get_modules_path().join("script.move"))
}

fn get_dfinance_dialect() -> DFinanceDialect {
    DFinanceDialect::default()
}

#[test]
fn test_show_compilation_errors() {
    let text = r"
script {
    use 0x0::Transaction;

    fun main() {
        let _ = Transaction::sender();
    }
}";
    let errors = get_dfinance_dialect()
        .compile_and_run(
            (get_script_path(), text.to_string()),
            &[],
            "0x111111111111111111111111".to_string(),
            vec![],
        )
        .unwrap_err()
        .downcast::<ExecCompilerError>()
        .unwrap()
        .0;
    assert_eq!(errors.len(), 1);
    assert_eq!(
        errors[0].parts[0].message,
        "Unbound module \'0x0::Transaction\'"
    );
}

#[test]
fn test_execute_custom_script_with_stdlib_module() {
    let text = r"
script {
    use 0x0::Transaction;

    fun main() {
        let _ = Transaction::sender();
    }
}";
    let deps = io::load_move_module_files(vec![get_stdlib_path()]).unwrap();
    get_dfinance_dialect()
        .compile_and_run(
            (existing_file_abspath(), text.to_string()),
            &deps,
            get_sender(),
            vec![],
        )
        .unwrap();
}

#[test]
fn test_execute_script_and_record_resource_changes() {
    let mut deps = io::load_move_module_files(vec![get_stdlib_path()]).unwrap();
    deps.push(get_record_module_dep());

    let script_text = r"
script {
    use 0x111111111111111111111111::Record;

    fun main() {
        let record = Record::create(10);
        Record::save(record);
    }
}";

    let changes = get_dfinance_dialect()
        .compile_and_run(
            (get_script_path(), script_text.to_string()),
            &deps,
            get_sender(),
            vec![],
        )
        .unwrap();
    assert_eq!(changes.len(), 1);

    assert_eq!(
        serde_json::to_value(&changes[0]).unwrap(),
        serde_json::json!({
            "ty": {
                "address": "0x000000000000000000000000111111111111111111111111",
                "module": "Record",
                "name": "T",
                "ty_args": [],
                "layout": ["U8", "U8"]
            },
            "op": {"type": "SetValue", "values": [10, 20]}
        })
    );
}

#[test]
fn test_execute_script_with_genesis_state_provided() {
    let _sender = get_sender();
    let mut deps = io::load_move_module_files(vec![get_stdlib_path()]).unwrap();
    deps.push(get_record_module_dep());

    let script_text = r"
script {
    use 0x111111111111111111111111::Record;

    fun main() {
        let record = Record::with_incremented_age();
        Record::save(record);
    }
}";

    let genesis = serde_json::json!([{
        "ty": {
            "address": "0x000000000000000000000000111111111111111111111111",
            "module": "Record",
            "name": "T",
            "ty_args": [],
            "layout": ["U8", "U8"]
        },
        "op": {"type": "SetValue", "values": [10, 20]}
    }]);
    let genesis_changes = serde_json::from_value(genesis).unwrap();
    let changes = get_dfinance_dialect()
        .compile_and_run(
            (get_script_path(), script_text.to_string()),
            &deps,
            get_sender(),
            genesis_changes,
        )
        .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        serde_json::to_value(&changes[0]).unwrap(),
        serde_json::json!({
            "ty": {
                "address": "0x000000000000000000000000111111111111111111111111",
                "module": "Record",
                "name": "T",
                "ty_args": [],
                "layout": ["U8", "U8"]
            },
            "op": {"type": "SetValue", "values": [11, 20]}
        })
    );
}

#[test]
fn missing_writesets_for_move_to_sender() {
    let module_text = r"
address 0x1 {
    module M {
        resource struct T { value: u8 }

        public fun get_t(v: u8) {
            move_to_sender<T>(T { value: v })
        }
    }
}
        ";
    let script_text = r"
script {
    fun main() {
        0x1::M::get_t(10);
    }
}
        ";
    let mut deps = io::load_move_module_files(vec![get_stdlib_path()]).unwrap();
    deps.push((
        leaked_fpath(get_modules_path().join("m.move")),
        module_text.to_string(),
    ));

    let sender = "0x1".to_string();
    let changes = get_dfinance_dialect()
        .compile_and_run(
            (get_script_path(), script_text.to_string()),
            &deps,
            sender,
            vec![],
        )
        .unwrap();
    assert_eq!(
        serde_json::to_value(changes).unwrap(),
        serde_json::json!([
          {
            "ty": {
              "address": "0x000000000000000000000000000000000000000000000001",
              "module": "M",
              "name": "T",
              "ty_args": [],
              "layout": [
                "U8"
              ]
            },
            "op": {
              "type": "SetValue",
              "values": [
                10
              ]
            }
          }
        ])
    );
}

#[test]
fn test_run_with_non_default_dfinance_dialect() {
    let module_source_text = r"
address wallet1me0cdn52672y7feddy7tgcj6j4dkzq2su745vh {
    module M {
        resource struct T { value: u8 }
        public fun get_t(v: u8) {
            move_to_sender<T>(T { value: v })
        }
    }
}
    ";
    let script_text = r"
script {
    fun main() {
        wallet1me0cdn52672y7feddy7tgcj6j4dkzq2su745vh::M::get_t(10);
    }
}
    ";

    let changes = compile_and_execute_script(
        (get_script_path(), script_text.to_string()),
        &[(
            leaked_fpath(get_modules_path().join("m.move")),
            module_source_text.to_string(),
        )],
        "dfinance",
        "wallet1me0cdn52672y7feddy7tgcj6j4dkzq2su745vh",
        serde_json::json!([]),
    )
    .unwrap();

    assert_eq!(
        changes,
        serde_json::json!([
          {
            "ty": {
              "address": "0xde5f86ce8ad7944f272d693cb4625a955b61015000000000",
              "module": "M",
              "name": "T",
              "ty_args": [],
              "layout": [
                "U8"
              ]
            },
            "op": {
              "type": "SetValue",
              "values": [
                10
              ]
            }
          }
        ])
    );
}
