diesel::table! {
    maintainers (id) {
        id -> Nullable<Integer>,
        contact -> Text,
        name -> Text,
    }
}

diesel::table! {
    package_maintainers (rowid) {
        rowid -> Integer,
        maintainer_id -> Integer,
        package_id -> Integer,
    }
}

diesel::table! {
    packages (id) {
        id -> Nullable<Integer>,
        rank -> Nullable<Integer>,
        pkg -> Nullable<Text>,
        pkg_id -> Text,
        pkg_name -> Text,
        pkg_family -> Nullable<Text>,
        pkg_type -> Nullable<Text>,
        pkg_webpage -> Nullable<Text>,
        app_id -> Nullable<Text>,
        description -> Nullable<Text>,
        version -> Text,
        version_upstream -> Nullable<Text>,
        licenses -> Nullable<Jsonb>,
        download_url -> Text,
        size -> Nullable<BigInt>,
        ghcr_pkg -> Nullable<Text>,
        ghcr_size -> Nullable<BigInt>,
        ghcr_blob -> Nullable<Text>,
        ghcr_url -> Nullable<Text>,
        checksum -> Nullable<Text>,
        icon -> Nullable<Text>,
        desktop -> Nullable<Text>,
        appstream -> Nullable<Text>,
        homepages -> Nullable<Jsonb>,
        notes -> Nullable<Jsonb>,
        source_urls -> Nullable<Jsonb>,
        tags -> Nullable<Jsonb>,
        categories -> Nullable<Jsonb>,
        build_id -> Nullable<Text>,
        build_date -> Nullable<Text>,
        build_action -> Nullable<Text>,
        build_script -> Nullable<Text>,
        build_log -> Nullable<Text>,
        provides -> Nullable<Jsonb>,
        snapshots -> Nullable<Jsonb>,
        replaces -> Nullable<Jsonb>,
        download_count -> Nullable<Integer>,
        download_count_week -> Nullable<Integer>,
        download_count_month -> Nullable<Integer>,
        soar_syms -> Bool,
        desktop_integration -> Nullable<Bool>,
        portable -> Nullable<Bool>,
        recurse_provides -> Nullable<Bool>,
    }
}

diesel::table! {
    repository (rowid) {
        rowid -> Integer,
        name -> Text,
        etag -> Text,
    }
}
