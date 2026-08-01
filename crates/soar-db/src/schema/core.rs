diesel::table! {
    packages (id) {
        id -> Integer,
        repo_name -> Text,
        pkg_id -> Nullable<Text>,
        pkg_name -> Text,
        pkg_family -> Nullable<Text>,
        pkg_type -> Nullable<Text>,
        version -> Text,
        size -> BigInt,
        checksum -> Nullable<Text>,
        installed_path -> Text,
        installed_date -> Text,
        profile -> Text,
        pinned -> Bool,
        is_installed -> Bool,
        detached -> Bool,
        unlinked -> Bool,
        provides -> Nullable<Jsonb>,
        install_patterns -> Nullable<Jsonb>,
        download_url -> Nullable<Text>,
        update_info -> Nullable<Text>,
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
