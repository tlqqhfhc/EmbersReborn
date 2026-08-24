pub mod physics;

use bevy::asset::AssetPath;
use bevy::prelude::*;
use bevy::scene::SceneFunction;
use rand::{SeedableRng, make_rng};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::result::Result;
use std::str::FromStr;
use std::sync::LazyLock;
use thiserror::Error;
use uuid::Uuid;

// TODO unit tests
#[macro_export]
macro_rules! path {
    (@build [$($result:expr),*] [] $lit:literal / $($rest:tt)*) => {
        path!(@build [$($result,)* $lit] [] $($rest)*)
    };
    (@build [$($result:expr),*] [] $lit:literal) => {
        path!(@concat $($result,)* $lit)
    };
    (@build [$($result:expr),*] [$($current:tt)+] / $($rest:tt)*) => {
        path!(@build [$($result,)* path!(@finish [$($current)+])] [] $($rest)*)
    };
    (@build [$($result:expr),*] [] / $($rest:tt)*) => {
        path!(@build [$($result),*] [] $($rest)*)
    };
    (@build [$($result:expr),*] [$($current:tt)*] $next:tt $($rest:tt)*) => {
        path!(@build [$($result),*] [$($current)* $next] $($rest)*)
    };
    (@build [$($result:expr),*] [$($current:tt)+]) => {
        path!(@concat $($result,)* path!(@finish [$($current)+]))
    };
    (@build [$($result:expr),*] []) => {
        path!(@concat $($result),*)
    };
    (@finish []) => { "" };
    (@finish [$($tokens:tt)+]) => { stringify!($($tokens)+) };
    (@concat) => { "" };
    (@concat $single:expr) => { $single };
    (@concat $first:expr, $($rest:expr),+) => {
        cfg_select! {
            unix => concat!($first, "/" , path!(@concat $($rest),+)),
            windows => concat!($first, "\\" , path!(@concat $($rest),+)),
        }
    };
    ($($tokens:tt)*) => {
        path!(@build [] [] $($tokens)*)
    };
}

pub trait Marker: Clone + Send + Sync + 'static {}

impl<T: Clone + Send + Sync + 'static> Marker for T {}

pub trait DynPartialCmp<Lhs, Rhs = Lhs> {
    fn dyn_eq(&self, lhs: Lhs, rhs: Rhs) -> bool;
    #[inline]
    fn dyn_ne(&self, lhs: Lhs, rhs: Rhs) -> bool {
        !self.dyn_eq(lhs, rhs)
    }
}

pub trait DynCmp<T>: DynPartialCmp<T, T> {}

pub trait Named {
    fn name(&self) -> &str;
}

pub trait UniquelyIdentified {
    fn unique_id(&self) -> Uuid;
}

pub trait Namespaced {
    fn namespace(&self) -> &str;
}

pub trait Keyed {
    fn key(&self) -> &NamespacedKey;
}

impl<T: Keyed + ?Sized> Keyed for Box<T> {
    fn key(&self) -> &NamespacedKey {
        self.as_ref().key()
    }
}

impl Keyed for NamespacedKey {
    fn key(&self) -> &NamespacedKey {
        self
    }
}

impl<T> Keyed for (NamespacedKey, T) {
    fn key(&self) -> &NamespacedKey {
        &self.0
    }
}

pub trait TypeKey {
    fn key() -> &'static NamespacedKey;
}

#[derive(Debug, Error)]
pub enum IllegalNamespacedKeyError {
    #[error("Invalid namespace: {0}")]
    IllegalNamespace(String),
    #[error("Invalid key: {0}")]
    IllegalKey(String),
    #[error("Invalid namespaced key: {0}")]
    IllegalNamespacedKey(String),
}

#[derive(Component, Serialize, Clone, Debug, Eq, Hash, PartialEq)]
#[serde(into = "String")]
pub struct NamespacedKey {
    namespaced_key: String,
    separator_index: usize,
}

impl<'de> Deserialize<'de> for NamespacedKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

pub static NAMESPACE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_]+)$").unwrap());
pub static KEY_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z0-9_/]+)$").unwrap());
pub static NAMESPACED_KEY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^(?P<namespace>[A-Za-z0-9_]+){}(?P<key>[A-Za-z0-9_/]+)$",
        NamespacedKey::SEPARATOR
    ))
    .unwrap()
});

impl NamespacedKey {
    pub const SEPARATOR: &'static str = ":";
    const SEPARATOR_LEN: usize = Self::SEPARATOR.len();
    pub(crate) const EMBERS_NAMESPACE: &'static str = "embers";
    #[inline]
    fn new_internal(namespace: &str, key: &str) -> Self {
        Self {
            namespaced_key: format!(
                "{}{separator}{}",
                namespace,
                key,
                separator = Self::SEPARATOR
            ),
            separator_index: namespace.len(),
        }
    }
    /// Creates a new [NamespacedKey] from the given `namespace` and `key`.
    ///
    /// # Panics
    /// This panics if the given `namespace` or `key` is invalid. If you don't want to implicitly panic, use [try_from](TryFrom<&str>::try_from).
    ///
    pub fn new<'namespace, 'key>(
        namespace: impl Into<&'namespace str>,
        key: impl Into<&'key str>,
    ) -> Self {
        let namespace = namespace.into();
        assert!(
            NAMESPACE_PATTERN.is_match(namespace),
            "Invalid namespace: {}",
            namespace
        );
        let key = key.into();
        assert!(KEY_PATTERN.is_match(key), "Invalid key: {}", key);
        Self::new_internal(namespace, key)
    }
    /// Creates a new [NamespacedKey] from the given `namespaced` and `key`.
    ///
    /// # Panics
    /// This panics if the given `key` is invalid. If you don't want to implicitly panic, use [try_from_with_namespaced](Self::try_from_with_namespaced).
    ///
    #[inline]
    pub fn new_namespaced<'key>(namespaced: &impl Namespaced, key: impl Into<&'key str>) -> Self {
        Self::new(namespaced.namespace(), key)
    }
    #[inline]
    pub(crate) fn new_embers(key: &str) -> Self {
        Self::new(Self::EMBERS_NAMESPACE, key)
    }
    /// Attempts to create a new [NamespacedKey] from the given `value`.
    pub fn try_from<'val>(value: impl Into<&'val str>) -> Result<Self, IllegalNamespacedKeyError> {
        let value = value.into();
        match NAMESPACED_KEY_PATTERN.captures(value) {
            Some(captures) => Ok(Self::new_internal(&captures["namespace"], &captures["key"])),
            None => Err(IllegalNamespacedKeyError::IllegalNamespacedKey(
                value.to_string(),
            )),
        }
    }
    /// Attempts to create a new [NamespacedKey] from the given `value`.
    ///
    /// If `value` does not contain a namespace, `default_namespace` is used.
    pub fn try_from_with<'val, 'default_namespace>(
        value: impl Into<&'val str>,
        default_namespace: impl Into<&'default_namespace str>,
    ) -> Result<Self, IllegalNamespacedKeyError> {
        let value = value.into();
        let namespaced = Self::try_from(value);
        if namespaced.is_ok() {
            return namespaced;
        }
        if !KEY_PATTERN.is_match(value) {
            return Err(IllegalNamespacedKeyError::IllegalKey(value.to_string()));
        }
        let default_namespace = default_namespace.into();
        if !NAMESPACE_PATTERN.is_match(default_namespace) {
            return Err(IllegalNamespacedKeyError::IllegalNamespace(
                default_namespace.to_string(),
            ));
        }
        Ok(Self::new_internal(default_namespace, value))
    }
    /// Attempts to create a new [NamespacedKey] from the given `value`.
    ///
    /// If `value` does not contain a namespace, the namespace of `default_namespace` is used.
    #[inline]
    pub fn try_from_with_namespaced<'val>(
        value: impl Into<&'val str>,
        default_namespace: &impl Namespaced,
    ) -> Result<Self, IllegalNamespacedKeyError> {
        Self::try_from_with(value, default_namespace.namespace())
    }
    #[inline]
    pub(crate) fn try_from_with_embers<'val>(
        value: impl Into<&'val str>,
    ) -> Result<Self, IllegalNamespacedKeyError> {
        Self::try_from_with(value, Self::EMBERS_NAMESPACE)
    }
    pub fn key(&self) -> &str {
        &self.namespaced_key[(self.separator_index + Self::SEPARATOR_LEN)..]
    }
    pub fn path_string(&self) -> String {
        format!("{}/{}", self.namespace(), self.key())
    }
}

impl Namespaced for NamespacedKey {
    fn namespace(&self) -> &str {
        &self.namespaced_key[..self.separator_index]
    }
}

impl fmt::Display for NamespacedKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.namespaced_key)
    }
}

impl From<NamespacedKey> for String {
    #[inline]
    fn from(value: NamespacedKey) -> Self {
        value.namespaced_key
    }
}

impl FromStr for NamespacedKey {
    type Err = IllegalNamespacedKeyError;
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        NamespacedKey::try_from_with_embers(s)
    }
}

impl From<&NamespacedKey> for AssetPath<'static> {
    fn from(value: &NamespacedKey) -> Self {
        Self::from(value.path_string())
    }
}

pub fn path_to_unix_components<P: AsRef<Path>>(path: P) -> String {
    use std::path::Component;
    let mut result = String::new();
    for component in path.as_ref().components() {
        match component {
            Component::Prefix(prefix) => {
                result.push_str(&prefix.as_os_str().to_string_lossy());
            }
            Component::RootDir => {
                if !result.ends_with('/') {
                    result.push('/');
                }
            }
            Component::CurDir => {
                if !result.is_empty() && !result.ends_with('/') {
                    result.push('/');
                }
                result.push('.');
            }
            Component::ParentDir => {
                if !result.is_empty() && !result.ends_with('/') {
                    result.push('/');
                }
                result.push_str("..");
            }
            Component::Normal(normal) => {
                if !result.is_empty() && !result.ends_with('/') {
                    result.push('/');
                }
                result.push_str(&normal.to_string_lossy());
            }
        }
    }
    result
}

#[derive(
    Asset,
    Clone,
    Copy,
    Debug,
    Eq,
    Event,
    Hash,
    Message,
    Ord,
    PartialEq,
    PartialOrd,
    Resource,
    TypePath,
)]
pub enum Void {}

#[derive(Resource)]
pub struct SystemRng<R: SeedableRng>(R);

impl<R: SeedableRng> Deref for SystemRng<R> {
    type Target = R;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R: SeedableRng> DerefMut for SystemRng<R> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<R: SeedableRng> FromWorld for SystemRng<R> {
    fn from_world(_world: &mut World) -> Self {
        SystemRng(make_rng())
    }
}

#[derive(Debug, Default)]
pub struct TextureAtlasManifest {
    textures_to_place: Vec<(Option<AssetId<Image>>, Handle<Image>)>,
}

#[derive(Debug, Error)]
pub enum TextureAtlasManifestError {
    #[error("The manifest held a handle({0:#?}) that referenced a nonexistent image")]
    InvalidTextureHandle(Handle<Image>),
}

impl TextureAtlasManifest {
    pub fn add_texture(
        &mut self,
        image_id: Option<AssetId<Image>>,
        texture: Handle<Image>,
    ) -> &mut Self {
        self.textures_to_place.push((image_id, texture));
        self
    }
    pub fn manifest<'img>(
        &self,
        images: &'img Assets<Image>,
    ) -> Result<TextureAtlasBuilder<'img>, TextureAtlasManifestError> {
        let mut builder = TextureAtlasBuilder::default();
        for (image_id, image) in self.textures_to_place.iter() {
            builder.add_texture(
                image_id.clone(),
                images.get(image).ok_or_else(|| {
                    TextureAtlasManifestError::InvalidTextureHandle(image.clone())
                })?,
            );
        }
        Ok(builder)
    }
}

pub fn template_bundle(
    template: impl Template<Output = impl Bundle> + Send + Sync + 'static,
) -> impl Scene {
    SceneFunction(move |_scene_context, resolved| {
        resolved.push_bundle_template(template);
    })
}

#[inline]
pub fn template_bundle_for(bundle: impl Bundle + Clone) -> impl Scene {
    template_bundle(template(move |_context| Ok(bundle.clone())))
}

pub fn remove_bundle<B: Bundle>() -> impl Scene {
    SceneFunction(|_scene_context, resolved| {
        resolved.push_bundle_template(template(|template_context| {
            template_context.entity.remove::<B>();
            Ok(())
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[derive(Debug)]
    struct DummyNamespaced(String);

    impl Namespaced for DummyNamespaced {
        fn namespace(&self) -> &str {
            &self.0
        }
    }

    #[test]
    fn namespace_pattern() {
        assert!(NAMESPACE_PATTERN.is_match("embers"));
        assert!(NAMESPACE_PATTERN.is_match("EmbErs"));
        assert!(NAMESPACE_PATTERN.is_match("0ember5"));
        assert!(NAMESPACE_PATTERN.is_match("_embers_"));
        assert!(!NAMESPACE_PATTERN.is_match(""));
        assert!(!NAMESPACE_PATTERN.is_match("not valid"));
        assert!(!NAMESPACE_PATTERN.is_match("not-valid"));
        assert!(!NAMESPACE_PATTERN.is_match("not.valid"));
        assert!(!NAMESPACE_PATTERN.is_match("not:valid"));
        assert!(!NAMESPACE_PATTERN.is_match("not/valid"));
        assert!(!NAMESPACE_PATTERN.is_match("不合法"));
    }

    #[test]
    fn key_pattern() {
        assert!(KEY_PATTERN.is_match("utils"));
        assert!(KEY_PATTERN.is_match("uTiLs"));
        assert!(KEY_PATTERN.is_match("ut1l5"));
        assert!(KEY_PATTERN.is_match("_utils_"));
        assert!(KEY_PATTERN.is_match("/path/to/utils"));
        assert!(!KEY_PATTERN.is_match(""));
        assert!(!KEY_PATTERN.is_match("not valid"));
        assert!(!KEY_PATTERN.is_match("not-valid"));
        assert!(!KEY_PATTERN.is_match("not.valid"));
        assert!(!KEY_PATTERN.is_match("not:valid"));
        assert!(!KEY_PATTERN.is_match("不合法"));
    }

    #[test]
    fn namespaced_key_pattern() {
        let captures = NAMESPACED_KEY_PATTERN.captures("embers:utils").unwrap();
        assert_eq!(captures.name("namespace").unwrap().as_str(), "embers");
        assert_eq!(captures.name("key").unwrap().as_str(), "utils");

        let captures = NAMESPACED_KEY_PATTERN.captures("_:__/_").unwrap();
        assert_eq!(captures.name("namespace").unwrap().as_str(), "_");
        assert_eq!(captures.name("key").unwrap().as_str(), "__/_");

        let captures = NAMESPACED_KEY_PATTERN
            .captures("998244353:0RdeR/0f/the_5tone")
            .unwrap();
        assert_eq!(captures.name("namespace").unwrap().as_str(), "998244353");
        assert_eq!(captures.name("key").unwrap().as_str(), "0RdeR/0f/the_5tone");

        assert!(!NAMESPACED_KEY_PATTERN.is_match(""));
        assert!(!NAMESPACED_KEY_PATTERN.is_match(":"));
        assert!(!NAMESPACED_KEY_PATTERN.is_match("embers:"));
        assert!(!NAMESPACED_KEY_PATTERN.is_match(":utils"));
        assert!(!NAMESPACED_KEY_PATTERN.is_match("embers_utils"));
        assert!(!NAMESPACED_KEY_PATTERN.is_match("embers:utils:namespaced_key"));
        assert!(!NAMESPACED_KEY_PATTERN.is_match("不合法"));
    }

    #[test]
    fn namespacing_keying() {
        let namespaced_key = NamespacedKey::new("embers", "utils");
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.separator_index, 6);
        assert_eq!(namespaced_key.key(), "utils");

        let namespaced_key =
            NamespacedKey::new_namespaced(&DummyNamespaced("embers".to_string()), "utils");
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.separator_index, 6);
        assert_eq!(namespaced_key.key(), "utils");

        let namespaced_key = NamespacedKey::try_from("embers:utils").unwrap();
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.separator_index, 6);
        assert_eq!(namespaced_key.key(), "utils");

        let namespaced_key = NamespacedKey::try_from_with("__/_", "_").unwrap();
        assert_eq!(namespaced_key.namespace(), "_");
        assert_eq!(namespaced_key.separator_index, 1);
        assert_eq!(namespaced_key.key(), "__/_");

        let namespaced_key = NamespacedKey::try_from_with("_:__/_", "default").unwrap();
        assert_eq!(namespaced_key.namespace(), "_");
        assert_eq!(namespaced_key.separator_index, 1);
        assert_eq!(namespaced_key.key(), "__/_");

        let namespaced_key = NamespacedKey::try_from_with_namespaced(
            "0RdeR/0f/the_5tone",
            &DummyNamespaced("998244353".to_string()),
        )
        .unwrap();
        assert_eq!(namespaced_key.namespace(), "998244353");
        assert_eq!(namespaced_key.separator_index, 9);
        assert_eq!(namespaced_key.key(), "0RdeR/0f/the_5tone");

        let namespaced_key = NamespacedKey::try_from_with_namespaced(
            "998244353:0RdeR/0f/the_5tone",
            &DummyNamespaced("default".to_string()),
        )
        .unwrap();
        assert_eq!(namespaced_key.namespace(), "998244353");
        assert_eq!(namespaced_key.separator_index, 9);
        assert_eq!(namespaced_key.key(), "0RdeR/0f/the_5tone");
    }

    #[test]
    fn newing_namespaced_key() {
        let key = NamespacedKey::new("embers", "utils");
        assert_eq!(key.namespace(), "embers");
        assert_eq!(key.key(), "utils");
        assert_eq!(key.to_string(), "embers:utils");
    }

    #[test]
    #[should_panic(expected = "Invalid namespace")]
    fn newing_namespaced_key_invalid_namespace() {
        NamespacedKey::new("not-valid", "utils");
    }

    #[test]
    #[should_panic(expected = "Invalid key")]
    fn newing_namespaced_key_invalid_key() {
        NamespacedKey::new("embers", "not-valid");
    }

    #[test]
    fn newing_namespaced_namespaced_key() {
        let namespaced_key =
            NamespacedKey::new_namespaced(&DummyNamespaced("embers".to_string()), "utils");
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.key(), "utils");
    }

    #[test]
    fn trying_from_namespaced_key_valid() {
        let namespaced_key = NamespacedKey::try_from("embers:utils").unwrap();
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.key(), "utils");
    }

    #[test]
    fn trying_from_namespaced_key_invalid() {
        assert!(NamespacedKey::try_from("embers:utils:namespaced_key").is_err());
        assert!(NamespacedKey::try_from("embers_utils").is_err());
        assert!(NamespacedKey::try_from("inval!d:utils").is_err());
    }

    #[test]
    fn trying_from_with_default_namespace_namespaced_key() {
        let namespaced_key = NamespacedKey::try_from_with("embers:utils", "default").unwrap();
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.key(), "utils");

        let namespaced_key = NamespacedKey::try_from_with("utils", "default").unwrap();
        assert_eq!(namespaced_key.namespace(), "default");
        assert_eq!(namespaced_key.key(), "utils");
    }

    #[test]
    fn trying_from_with_namespaced_key_invalid() {
        let result = NamespacedKey::try_from_with("inv@lid", "default");
        assert!(result.is_err());

        let result = NamespacedKey::try_from_with("item", "inv@lid-ns");
        assert!(result.is_err());
    }

    #[test]
    fn trying_from_with_namespaced_namespaced_key() {
        let namespaced = DummyNamespaced("default".to_string());

        let namespaced_key = NamespacedKey::try_from_with_namespaced("utils", &namespaced).unwrap();
        assert_eq!(namespaced_key.namespace(), "default");
        assert_eq!(namespaced_key.key(), "utils");

        let namespaced_key =
            NamespacedKey::try_from_with_namespaced("embers:utils", &namespaced).unwrap();
        assert_eq!(namespaced_key.namespace(), "embers");
        assert_eq!(namespaced_key.key(), "utils");
    }

    #[test]
    fn namespaced_key_displaying_and_stringifying() {
        let key = NamespacedKey::new("embers", "utils");
        assert_eq!(format!("{}", key), "embers:utils");
        assert_eq!(
            <NamespacedKey as Into<String>>::into(key.clone()),
            "embers:utils"
        );
    }

    #[test]
    fn namespaced_key_equality_and_hashing() {
        let namespaced_key1 = NamespacedKey::new("embers", "utils");
        let namespaced_key2 = NamespacedKey::new("embers", "utils");
        let namespaced_key3 = NamespacedKey::new("embers", "util");
        let namespaced_key4 = NamespacedKey::try_from("embers:utils").unwrap();

        assert_eq!(namespaced_key1, namespaced_key2);
        assert_eq!(namespaced_key1, namespaced_key4);
        assert_ne!(namespaced_key1, namespaced_key3);

        let mut set = HashSet::new();
        set.insert(namespaced_key1.clone());
        set.insert(namespaced_key2.clone());
        assert_eq!(set.len(), 1);
        set.insert(namespaced_key3);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn keyed_tuples() {
        let namespaced_key = NamespacedKey::new("embers", "utils");
        assert_eq!((namespaced_key.clone(), 42).key(), &namespaced_key);
    }
}
