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
            ) -> Result<u64, $crate::models::gen::GenerationError> {
                let current = self.generation();
                if current < generation {
                    self.$f = generation;
                    Ok(generation)
                } else {
                    Err($crate::models::gen::GenerationError::NotIncrementing {
                        current,
                        desired: generation,
                    })
                }
            }
        }
    };
}

#[derive(Debug, Error)]
pub enum GenerationError {
    #[error("Generation not incrementing (was: {current}, desired: {desired})")]
    NotIncrementing { current: u64, desired: u64 },
}

pub trait Generation {
    /// Get the generation from the provided resource, increment it by one, and set it to this resource.
    fn set_incremented_generation(
        &mut self,
        current: &dyn Generation,
    ) -> Result<u64, GenerationError> {
        self.set_generation(current.generation() + 1)
    }

    /// Increment the generation of this resource by one
    fn increment_generation(&mut self) -> Result<u64, GenerationError> {
        self.set_generation(self.generation() + 1)
    }

    /// Get the current generation
    fn generation(&self) -> u64;

    /// Set the generation
    fn set_generation(&mut self, generation: u64) -> Result<u64, GenerationError>;
}
