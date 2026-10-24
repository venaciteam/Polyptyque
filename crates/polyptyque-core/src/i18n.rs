//! Traduction des messages (voir « Langues et formats » dans le cahier des
//! charges).
//!
//! Le français est la langue du code : chaque texte affiché est écrit en
//! français dans `tr!("…")`, et le catalogue anglais (format gettext `.po`)
//! donne sa version anglaise. Le crate `tr` range un catalogue **par crate** :
//! chaque crate qui affiche des messages expose donc sa propre fonction
//! `set_catalog`, que l'interface appelle pour toutes au changement de langue.

use std::borrow::Cow;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tr::{PoTranslator, Translator};

/// Catalogue d'une langue, lu depuis le contenu d'un fichier `.po`.
pub struct Catalog(PoTranslator);

impl Catalog {
    pub fn parse(po: &[u8]) -> Result<Self> {
        PoTranslator::from_vec_u8(po.to_vec()).map(Self).map_err(|e| anyhow!("catalogue de traduction illisible : {e}"))
    }
}

impl Translator for Catalog {
    fn translate<'a>(&'a self, string: &'a str, context: Option<&'a str>) -> Cow<'a, str> {
        // Un texte pas encore traduit (msgstr vide) garde sa version
        // française plutôt que de disparaître de l'écran.
        let text = self.0.translate(string, context);
        if text.is_empty() {
            Cow::Borrowed(string)
        } else {
            text
        }
    }

    fn ntranslate<'a>(&'a self, n: u64, singular: &'a str, plural: &'a str, context: Option<&'a str>) -> Cow<'a, str> {
        let text = self.0.ntranslate(n, singular, plural, context);
        if text.is_empty() {
            Cow::Borrowed(if n > 1 { plural } else { singular })
        } else {
            text
        }
    }
}

/// Le français, langue du code : les textes restent tels quels, avec la
/// règle du pluriel française (« 0 fichier », « 1 fichier », « 2 fichiers »),
/// que `tr` n'applique pas de lui-même.
pub struct French;

impl Translator for French {
    fn translate<'a>(&'a self, string: &'a str, _context: Option<&'a str>) -> Cow<'a, str> {
        Cow::Borrowed(string)
    }

    fn ntranslate<'a>(&'a self, n: u64, singular: &'a str, plural: &'a str, _context: Option<&'a str>) -> Cow<'a, str> {
        Cow::Borrowed(if n > 1 { plural } else { singular })
    }
}

/// Langue des messages de cette crate : `None` pour le français.
pub fn set_catalog(catalog: Option<Arc<Catalog>>) {
    match catalog {
        Some(catalog) => tr::set_translator!(catalog),
        None => tr::set_translator!(French),
    }
}
