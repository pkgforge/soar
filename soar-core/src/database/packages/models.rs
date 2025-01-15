#[derive(Debug, Clone, Copy)]
pub enum PackageSort {
    Id,
    PackageName,
    Family,
    FamilyAndPackage,
}

impl PackageSort {
    pub fn get_order_clause(&self) -> &'static str {
        match self {
            PackageSort::Id => "p.id",
            PackageSort::PackageName => "p.pkg_name, p.id",
            PackageSort::Family => "f.value, p.id",
            PackageSort::FamilyAndPackage => "f.value, p.pkg_name, p.id",
        }
    }

    pub fn get_next_page_condition(&self) -> &'static str {
        match self {
            PackageSort::Id => "p.id > ?",
            PackageSort::PackageName => "(p.pkg_name, p.id) > (?, ?)",
            PackageSort::Family => "(f.value, p.id) > (?, ?)",
            PackageSort::FamilyAndPackage => "(f.value, p.pkg_name, p.id) > (?, ?, ?)",
        }
    }

    pub fn bind_pagination_params(
        &self,
        params: &mut Vec<Box<dyn rusqlite::ToSql>>,
        state: &IterationState,
    ) {
        match self {
            Self::PackageName => {
                let pkg_name = state.pkg_name.clone().unwrap_or_default();
                params.push(Box::new(pkg_name));
            }
            Self::Family => {
                let family = state.family.clone().unwrap_or_default();
                params.push(Box::new(family));
            }
            Self::FamilyAndPackage => {
                let family = state.family.clone().unwrap_or_default();
                let pkg_name = state.pkg_name.clone().unwrap_or_default();
                params.push(Box::new(family));
                params.push(Box::new(pkg_name));
            }
            _ => {}
        }

        params.push(Box::new(state.id));
    }
}

#[derive(Debug, Default, Clone)]
pub struct PackageFilter {
    pub repo_name: Option<String>,
    pub pkg_name: Option<String>,
    pub exact_pkg_name: Option<String>,
    pub family: Option<String>,
    pub search_term: Option<String>,
    pub exact_case: bool,
}

#[derive(Debug, Default, Clone)]
pub struct IterationState {
    pub id: u64,
    pub pkg_name: Option<String>,
    pub family: Option<String>,
}
