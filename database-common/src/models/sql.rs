use crate::models::Lock;
use drogue_cloud_service_api::{
    auth::user::{IsAdmin, UserInformation},
    labels::Operation,
};
use tokio_postgres::types::{ToSql, Type};

pub fn slice_iter<'a>(
    s: &'a [&'a (dyn ToSql + Sync)],
) -> impl ExactSizeIterator<Item = &'a dyn ToSql> + 'a {
    s.iter().map(|s| *s as _)
}

pub struct SelectBuilder<'a> {
    select: String,
    params: Vec<&'a (dyn ToSql + Sync + 'a)>,
    types: Vec<Type>,

    have_where: bool,
    sort: Vec<String>,
    lock: Lock,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl<'a> SelectBuilder<'a> {
    pub fn new<S: Into<String>>(
        select: S,
        params: Vec<&'a (dyn ToSql + Sync)>,
        types: Vec<Type>,
    ) -> Self {
        Self {
            select: select.into(),
            params,
            types,
            have_where: false,
            lock: Lock::None,
            sort: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    /// Marks the select as already having a WHERE clause
    pub fn has_where(mut self) -> Self {
        self.have_where = true;
        self
    }

    #[inline]
    fn ensure_where_or_and(&mut self) {
        if !self.have_where {
            self.have_where = true;
            self.select.push_str("\nWHERE");
        } else {
            self.select.push_str("\nAND");
        }
    }

    pub fn limit(mut self, limit: Option<usize>) -> Self {
        self.limit = limit;
        self
    }

    pub fn offset(mut self, offset: Option<usize>) -> Self {
        self.offset = offset;
        self
    }

    pub fn lock(mut self, lock: Lock) -> Self {
        self.lock = lock;
        self
    }

    pub fn sort<I>(mut self, sort: I) -> Self
    where
        I: IntoIterator + Sized,
        I::Item: ToString,
    {
        self.sort = sort.into_iter().map(|i| i.to_string()).collect();
        self
    }

    /// Add restrictions to the select so that no unauthorized items get returned for the read permission.
    ///
    /// NOTE: This must be aligned with [`crate::auth::authorize`].
    pub fn auth_read(mut self, user: &'a Option<&'a UserInformation>) -> Self {
        // check if we have authentication enabled
        let user = match user {
            Some(user) => user,
            None => return self,
        };

        // check if we are admin
        if user.is_admin() {
            // early return as we are admin
            return self;
        }

        // ensure we have a "where" or "and"

        self.ensure_where_or_and();

        // prepare the user id

        match user {
            UserInformation::Authenticated(user) => {
                self.params.push(&user.user_id);
                self.types.push(Type::VARCHAR);
            }
            UserInformation::Anonymous => {
                self.params.push(&"");
                self.types.push(Type::VARCHAR);
            }
        }
        let idx = self.params.len();

        // must be equal to the owner (which may be empty)
        // or contain a member with one of the roles eligible for reading
        // or contain the "anonymous" member

        self.select.push_str(&format!(
            r#"
    (
        OWNER=${idx}
    OR
        MEMBERS->${idx}->>'role' IN ('reader', 'manager', 'admin')
    OR
        MEMBERS->''->>'role' IN ('reader', 'manager', 'admin')
    )
"#,
            idx = idx
        ));

        // done

        self
    }

    /// Add a name filter.
    pub fn name(mut self, name: &'a Option<&'a str>) -> Self {
        if let Some(name) = name.as_ref() {
            self.ensure_where_or_and();
            self.params.push(name);
            self.types.push(Type::VARCHAR);
            self.select
                .push_str(&format!(" NAME=${}", self.params.len()));
        }
        self
    }

    /// Add a labels filter.
    pub fn labels(mut self, labels: &'a [Operation]) -> Self {
        for op in labels {
            self.ensure_where_or_and();
            match op {
                Operation::Exists(label) => {
                    self.params.push(label);
                    self.types.push(Type::VARCHAR);
                    self.select
                        .push_str(&format!(" LABELS ? ${}", self.params.len()));
                }
                Operation::NotExists(label) => {
                    self.params.push(label);
                    self.types.push(Type::VARCHAR);
                    self.select
                        .push_str(&format!(" (NOT LABELS ? ${})", self.params.len()));
                }
                Operation::Eq(label, value) => {
                    self.params.push(label);
                    self.params.push(value);
                    self.types.push(Type::VARCHAR);
                    self.types.push(Type::VARCHAR);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} = ${}",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
                Operation::NotEq(label, value) => {
                    self.params.push(label);
                    self.params.push(value);
                    self.types.push(Type::VARCHAR);
                    self.types.push(Type::VARCHAR);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} <> ${}",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
                Operation::In(label, values) => {
                    self.params.push(label);
                    self.params.push(values);
                    self.types.push(Type::VARCHAR);
                    self.types.push(Type::VARCHAR_ARRAY);
                    self.select.push_str(&format!(
                        " LABELS ->> ${} = ANY (${})",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
                Operation::NotIn(label, values) => {
                    self.params.push(label);
                    self.params.push(values);
                    self.types.push(Type::VARCHAR);
                    self.types.push(Type::VARCHAR_ARRAY);
                    self.select.push_str(&format!(
                        " NOT(LABELS ->> ${} = ANY (${}))",
                        self.params.len() - 1,
                        self.params.len()
                    ));
                }
            }
        }
        self
    }

    pub fn build(self) -> (String, Vec<&'a (dyn ToSql + Sync)>, Vec<Type>) {
        let mut select = self.select;

        if !self.sort.is_empty() {
            select.push_str("\nORDER BY ");
            select.push_str(&self.sort.join(","));
        }

        if let Some(limit) = self.limit {
            select.push_str(&format!("\nLIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            select.push_str(&format!("\nOFFSET {}", offset));
        }

        select.push('\n');
        // append after the where
        select.push_str(self.lock.as_ref());

        // return result
        (select, self.params, self.types)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use drogue_cloud_service_api::labels::LabelSelector;
    use std::convert::TryInto;
    use std::fmt::Debug;

    #[test]
    fn test_to_sql_1() {
        let builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new(), Vec::new());

        let (sql, params, types) = builder.build();

        assert_eq!(sql, "SELECT * FROM TABLE\n");
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            Vec::<String>::new()
        );
        assert_eq!(types, vec![]);
    }

    #[test]
    fn test_to_sql_name() {
        let mut builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new(), Vec::new());

        builder = builder.name(&Some("Foo"));

        let (sql, params, types) = builder.build();

        assert_eq!(sql, "SELECT * FROM TABLE\nWHERE NAME=$1\n");
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            to_debug(&[&"Foo"])
        );
        assert_eq!(types, vec![Type::VARCHAR]);
    }

    #[test]
    fn test_to_labels_1() {
        let mut builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new(), Vec::new());

        let selector: LabelSelector = r#"foo,bar"#.try_into().unwrap();
        builder = builder.labels(&selector.0);

        let (sql, params, types) = builder.build();

        assert_eq!(
            sql,
            r#"SELECT * FROM TABLE
WHERE LABELS ? $1
AND LABELS ? $2
"#
        );
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            to_debug(&[&"foo", &"bar"])
        );
        assert_eq!(types, vec![Type::VARCHAR, Type::VARCHAR]);
    }

    #[test]
    fn test_to_labels_2() {
        let mut builder = SelectBuilder::new("SELECT * FROM TABLE", Vec::new(), Vec::new());

        let selector: LabelSelector = r#"!foo,bar in (f1, f2, f3), baz!=abc"#.try_into().unwrap();
        builder = builder.labels(&selector.0);

        let (sql, params, types) = builder.build();

        assert_eq!(
            sql,
            r#"SELECT * FROM TABLE
WHERE (NOT LABELS ? $1)
AND LABELS ->> $2 = ANY ($3)
AND LABELS ->> $4 <> $5
"#
        );
        assert_eq!(
            params
                .into_iter()
                .map(|p| format!("{:?}", p))
                .collect::<Vec<String>>(),
            to_debug(&[&"foo", &"bar", &["f1", "f2", "f3"], &"baz", &"abc"])
        );
        assert_eq!(
            types,
            vec![
                Type::VARCHAR,
                Type::VARCHAR,
                Type::VARCHAR_ARRAY,
                Type::VARCHAR,
                Type::VARCHAR
            ]
        );
    }

    fn to_debug(list: &[&dyn Debug]) -> Vec<String> {
        list.iter().map(|s| format!("{:?}", s)).collect()
    }
}
