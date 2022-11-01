/// Injects a second variable into the function call.
/// `inject(f, b) = |a| f(a, b)`.
pub fn inject<F, A, B, R>(f: F, b: B) -> impl Fn(A) -> R
where
    F: Fn(A, B) -> R,
    B: Copy,
{
    move |a| f(a, b)
}
