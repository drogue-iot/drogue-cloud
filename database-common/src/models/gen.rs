use thiserror::Error;

#[macro_export]
macro_rules! generation {
    ($t:ty => $f:ident) => {
        impl $crate::models::gen::Generation for $t {
            #[inline]
            fn generation(&self) -> u64 {
                self.$f
            }

            fn set_generation(
                &mut self,
                generation: u64,
            ) -> Result<u64, $crate::models::gen::AdvanceError> {
                let current = self.generation();
                if current < generation {
                    self.$f = generation;
                    Ok(generation)
                } else {
                    Err($crate::models::gen::AdvanceError::NotIncrementing {
                        current,
                        desired: generation,
                    })
                }
            }
        }
    };
}

#[macro_export]
macro_rules! revision {
    ($t:ty => $f:ident) => {
        impl $crate::models::gen::Revision for $t {
            #[inline]
            fn revision(&self) -> u64 {
                self.$f
            }

            fn set_revision(
                &mut self,
                revision: u64,
            ) -> Result<u64, $crate::models::gen::AdvanceError> {
                let current = self.revision();
                if current < revision {
                    self.$f = revision;
                    Ok(revision)
                } else {
                    Err($crate::models::gen::AdvanceError::NotIncrementing {
                        current,
                        desired: revision,
                    })
                }
            }
        }
    };
}

#[derive(Debug, Error)]
pub enum AdvanceError {
    #[error("not incrementing (was: {current}, desired: {desired})")]
    NotIncrementing { current: u64, desired: u64 },
}

pub trait Advance {
    /// Advance the generation by one for a spec change, and advance the revision in any case.
    fn advance_from<S>(&mut self, paths: &[String], current: &S) -> Result<u64, AdvanceError>
    where
        S: Generation + Revision;

    /// Increment the revision of this resource by one
    fn advance_revision(&mut self) -> Result<u64, AdvanceError>;
}

impl<T> Advance for T
where
    T: Generation + Revision,
{
    fn advance_from<S>(&mut self, paths: &[String], current: &S) -> Result<u64, AdvanceError>
    where
        S: Generation + Revision,
    {
        let result = self.set_revision(current.revision() + 1);

        for path in paths {
            if path.starts_with(".spec") {
                self.set_generation(current.generation() + 1)?;
                break;
            }
        }

        result
    }

    fn advance_revision(&mut self) -> Result<u64, AdvanceError> {
        self.set_revision(self.revision() + 1)
    }
}

pub trait Generation {
    /// Get the current generation
    fn generation(&self) -> u64;

    /// Set the generation
    fn set_generation(&mut self, generation: u64) -> Result<u64, AdvanceError>;
}

pub trait Revision {
    /// Get the current revision
    fn revision(&self) -> u64;

    /// Set the revision
    fn set_revision(&mut self, revision: u64) -> Result<u64, AdvanceError>;
}
