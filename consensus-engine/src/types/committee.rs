use std::collections::BTreeSet;

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommitteeId(pub(crate) [u8; 32]);

impl CommitteeId {
    pub const fn new(val: [u8; 32]) -> Self {
        Self(val)
    }
}

impl From<[u8; 32]> for CommitteeId {
    fn from(id: [u8; 32]) -> Self {
        Self(id)
    }
}

impl From<&[u8; 32]> for CommitteeId {
    fn from(id: &[u8; 32]) -> Self {
        Self(*id)
    }
}

impl From<CommitteeId> for [u8; 32] {
    fn from(id: CommitteeId) -> Self {
        id.0
    }
}

impl core::fmt::Display for CommitteeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x")?;
        for v in self.0 {
            write!(f, "{:02x}", v)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
#[repr(transparent)]
pub struct Committee {
    members: BTreeSet<CommitteeId>,
}

impl Committee {
    #[inline]
    pub const fn new() -> Self {
        Self {
            members: BTreeSet::new(),
        }
    }

    #[inline]
    pub fn hash<D: digest::Digest>(
        &self,
    ) -> digest::generic_array::GenericArray<u8, <D as digest::OutputSizeUser>::OutputSize> {
        let mut hasher = D::new();
        for member in &self.members {
            hasher.update(member.0);
        }
        hasher.finalize()
    }
}

impl<'a, T> From<T> for Committee
where
    T: Iterator<Item = &'a CommitteeId>,
{
    fn from(members: T) -> Self {
        Self {
            members: members.into_iter().copied().collect(),
        }
    }
}

impl core::iter::FromIterator<[u8; 32]> for Committee {
    fn from_iter<T: IntoIterator<Item = [u8; 32]>>(iter: T) -> Self {
        Self {
            members: iter.into_iter().map(CommitteeId).collect(),
        }
    }
}

impl<'a> core::iter::FromIterator<&'a [u8; 32]> for Committee {
    fn from_iter<T: IntoIterator<Item = &'a [u8; 32]>>(iter: T) -> Self {
        Self {
            members: iter.into_iter().copied().map(CommitteeId).collect(),
        }
    }
}

impl core::iter::FromIterator<CommitteeId> for Committee {
    fn from_iter<T: IntoIterator<Item = CommitteeId>>(iter: T) -> Self {
        Self {
            members: iter.into_iter().collect(),
        }
    }
}

impl core::iter::IntoIterator for Committee {
    type Item = CommitteeId;

    type IntoIter = std::collections::btree_set::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.members.into_iter()
    }
}

impl<'a> core::iter::IntoIterator for &'a Committee {
    type Item = &'a CommitteeId;

    type IntoIter = std::collections::btree_set::Iter<'a, CommitteeId>;

    fn into_iter(self) -> Self::IntoIter {
        self.members.iter()
    }
}

impl<'a> FromIterator<&'a CommitteeId> for Committee {
    fn from_iter<T: IntoIterator<Item = &'a CommitteeId>>(iter: T) -> Self {
        Self {
            members: iter.into_iter().copied().collect(),
        }
    }
}

impl core::ops::Deref for Committee {
    type Target = BTreeSet<CommitteeId>;

    fn deref(&self) -> &Self::Target {
        &self.members
    }
}

impl core::ops::DerefMut for Committee {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.members
    }
}