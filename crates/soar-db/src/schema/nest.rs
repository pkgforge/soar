diesel::table! {
    packages (id) {
        id -> Nullable<Integer>,
        name -> Text,
        url -> Text,
    }
}
