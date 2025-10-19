diesel::table! {
    packages (id) {
        id -> Nullable<Integer>,
        repo_name -> Text,
        pkg -> Nullable<Text>,
        pkg_id -> Text,
        pkg_name -> Text,
        pkg_type -> Nullable<Text>,
        version -> Text,
        size -> BigInt,
        checksum -> Nullable<Text>,
        installed_path -> Text,
        installed_date -> Text,
        profile -> Text,
        pinned -> Bool,
        is_installed -> Bool,
        with_pkg_id -> Bool,
        detached -> Bool,
        unlinked -> Bool,
        provides -> Nullable<Jsonb>,
        install_patterns -> Nullable<Jsonb>,
    }

}

diesel::table! {
    portable_package (rowid) {
        rowid -> Integer,
        package_id -> Integer,
        portable_path -> Nullable<Text>,
        portable_home -> Nullable<Text>,
        portable_config -> Nullable<Text>,
        portable_share -> Nullable<Text>,
        portable_cache -> Nullable<Text>,
    }
}

diesel::joinable!(portable_package -> packages (package_id));

diesel::allow_tables_to_appear_in_same_query!(packages, portable_package,);
