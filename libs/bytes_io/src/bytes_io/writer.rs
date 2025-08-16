// rust std imports
use std::borrow::Cow;
use std::io::{self, Write};
// external imports
use bytemuck::{cast_slice, Pod};
use encoding_rs::{Encoding, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252};
use hashbrown::HashMap;
use memchr::memchr;
use nalgebra::{allocator::Allocator, DefaultAllocator, Dim, OMatrix, Scalar};
use smart_default::SmartDefault;
// internal imports
use crate::Save;

#[derive(Debug, SmartDefault)]
pub struct Writer {
    pub cursor: io::Cursor<Vec<u8>>,
    pub context: HashMap<u64, u64>,
    #[default(WINDOWS_1252)]
    pub encoding: &'static Encoding,
}

impl Writer {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            cursor: io::Cursor::new(bytes),
            ..Default::default()
        }
    }

    /// Create a new Writer with Windows-1252 encoding (Western European)
    pub fn new_windows_1252(bytes: Vec<u8>) -> Self {
        Self {
            cursor: io::Cursor::new(bytes),
            encoding: WINDOWS_1252,
            ..Default::default()
        }
    }

    /// Create a new Writer with Windows-1250 encoding (Central European)
    pub fn new_windows_1250(bytes: Vec<u8>) -> Self {
        Self {
            cursor: io::Cursor::new(bytes),
            encoding: WINDOWS_1250,
            ..Default::default()
        }
    }

    /// Create a new Writer with Windows-1251 encoding (Cyrillic/Russian)
    pub fn new_windows_1251(bytes: Vec<u8>) -> Self {
        Self {
            cursor: io::Cursor::new(bytes),
            encoding: WINDOWS_1251,
            ..Default::default()
        }
    }

    /// Set the encoding to Windows-1252
    pub fn set_windows_1252(&mut self) {
        self.encoding = WINDOWS_1252;
    }

    /// Set the encoding to Windows-1250 (Central European)
    pub fn set_windows_1250(&mut self) {
        self.encoding = WINDOWS_1250;
    }

    /// Set the encoding to Windows-1251 (Cyrillic/Russian)
    pub fn set_windows_1251(&mut self) {
        self.encoding = WINDOWS_1251;
    }

    /// Auto-detect and set the appropriate encoding based on the string content
    /// Returns the encoding that was selected
    pub fn auto_set_encoding(&mut self, text: &str) -> &'static Encoding {
        // Try Windows-1252 first (most common for Western languages)
        if let (_, _, false) = WINDOWS_1252.encode(text) {
            self.encoding = WINDOWS_1252;
            return WINDOWS_1252;
        }
        
        // Check for Cyrillic characters (Russian, etc.)
        if text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)) {
            if let (_, _, false) = WINDOWS_1251.encode(text) {
                self.encoding = WINDOWS_1251;
                return WINDOWS_1251;
            }
        }
        
        // Check for Central European characters (Polish, Czech, Slovak, Hungarian, etc.)
        if text.chars().any(|c| {
            matches!(c, 
                'Ą' | 'ą' | 'Ć' | 'ć' | 'Ę' | 'ę' | 'Ł' | 'ł' | 'Ń' | 'ń' | 'Ó' | 'ó' | 
                'Ś' | 'ś' | 'Ź' | 'ź' | 'Ż' | 'ż' | // Polish
                'Č' | 'č' | 'Ď' | 'ď' | 'Ň' | 'ň' | 'Ř' | 'ř' | 'Š' | 'š' | 'Ť' | 'ť' | 
                'Ů' | 'ů' | 'Ž' | 'ž' | // Czech
                'Ľ' | 'ľ' | 'Ŕ' | 'ŕ' | // Slovak
                'Ő' | 'ő' | 'Ű' | 'ű' | // Hungarian
                'Đ' | 'đ' // Croatian/Serbian Latin
            )
        }) {
            if let (_, _, false) = WINDOWS_1250.encode(text) {
                self.encoding = WINDOWS_1250;
                return WINDOWS_1250;
            }
        }
        
        // Try Windows-1250 if not already tried
        if let (_, _, false) = WINDOWS_1250.encode(text) {
            self.encoding = WINDOWS_1250;
            return WINDOWS_1250;
        }
        
        // Try Windows-1251 if not already tried
        if let (_, _, false) = WINDOWS_1251.encode(text) {
            self.encoding = WINDOWS_1251;
            return WINDOWS_1251;
        }
        
        // Default to Windows-1252 if all fail
        self.encoding = WINDOWS_1252;
        WINDOWS_1252
    }

    /// Try to encode a string with the best fitting encoding between Windows-1252, Windows-1250, and Windows-1251
    /// Returns the encoded bytes and the encoding used
    pub fn encode_with_best_fit<'a>(&self, text: &'a str) -> io::Result<(Cow<'a, [u8]>, &'static Encoding)> {
        // Try current encoding first
        if let (bytes, _, false) = self.encoding.encode(text) {
            return Ok((self.process_encoded_bytes(bytes), self.encoding));
        }

        // Try Windows-1252 if not current encoding (most common)
        if self.encoding != WINDOWS_1252 {
            if let (bytes, _, false) = WINDOWS_1252.encode(text) {
                return Ok((self.process_encoded_bytes(bytes), WINDOWS_1252));
            }
        }

        // Check for Cyrillic characters and try Windows-1251
        if text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)) {
            if self.encoding != WINDOWS_1251 {
                if let (bytes, _, false) = WINDOWS_1251.encode(text) {
                    return Ok((self.process_encoded_bytes(bytes), WINDOWS_1251));
                }
            }
        }

        // Check for Central European characters and try Windows-1250
        if text.chars().any(|c| {
            matches!(c, 
                'Ą' | 'ą' | 'Ć' | 'ć' | 'Ę' | 'ę' | 'Ł' | 'ł' | 'Ń' | 'ń' | 'Ó' | 'ó' | 
                'Ś' | 'ś' | 'Ź' | 'ź' | 'Ż' | 'ż' | // Polish
                'Č' | 'č' | 'Ď' | 'ď' | 'Ň' | 'ň' | 'Ř' | 'ř' | 'Š' | 'š' | 'Ť' | 'ť' | 
                'Ů' | 'ů' | 'Ž' | 'ž' | // Czech
                'Ľ' | 'ľ' | 'Ŕ' | 'ŕ' | // Slovak
                'Ő' | 'ő' | 'Ű' | 'ű' | // Hungarian
                'Đ' | 'đ' // Croatian/Serbian Latin
            )
        }) {
            if self.encoding != WINDOWS_1250 {
                if let (bytes, _, false) = WINDOWS_1250.encode(text) {
                    return Ok((self.process_encoded_bytes(bytes), WINDOWS_1250));
                }
            }
        }

        // Try remaining encodings if not already tried
        if self.encoding != WINDOWS_1250 {
            if let (bytes, _, false) = WINDOWS_1250.encode(text) {
                return Ok((self.process_encoded_bytes(bytes), WINDOWS_1250));
            }
        }

        if self.encoding != WINDOWS_1251 {
            if let (bytes, _, false) = WINDOWS_1251.encode(text) {
                return Ok((self.process_encoded_bytes(bytes), WINDOWS_1251));
            }
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData, 
            format!("Cannot encode '{}' with Windows-1252, Windows-1250, or Windows-1251", text)
        ))
    }

    /// Helper method to process encoded bytes (remove null terminators)
    fn process_encoded_bytes<'a>(&self, bytes: Cow<'a, [u8]>) -> Cow<'a, [u8]> {
        match memchr(0, &bytes) {
            None => bytes,
            Some(i) => match bytes {
                Cow::Borrowed(s) => s[..i].into(),
                Cow::Owned(mut s) => {
                    s.truncate(i);
                    s.into()
                }
            },
        }
    }

    pub fn error<M>(message: M) -> io::Result<()>
    where
        M: Into<Cow<'static, str>>,
    {
        Err(io::Error::new(io::ErrorKind::InvalidData, message.into()))
    }

    pub fn save<S>(&mut self, value: &S) -> io::Result<()>
    where
        S: Save,
    {
        value.save(self)
    }

    pub fn save_as<T, S>(&mut self, value: T) -> io::Result<()>
    where
        S: Save + TryFrom<T>,
    {
        match S::try_from(value) {
            Ok(value) => value.save(self),
            _ => Self::error("Invalid Save Conversion"),
        }
    }

    pub fn save_bytes(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.write_all(bytes)
    }

    pub fn save_string(&mut self, value: &str) -> io::Result<()> {
        if value.is_empty() {
            // save the string size
            self.save(&1u32)?;
            // save null terminator
            self.save(&0u8)?;
            return Ok(());
        }

        if let (bytes, _, false) = self.encoding.encode(value) {
            // scan for null terminator
            if let Some(index) = memchr(0, &bytes) {
                // save the string size
                self.save_as::<usize, u32>(index)?;
                // save the string data
                self.save_bytes(&bytes[..index])?;
            } else {
                // save the string size
                self.save_as::<usize, u32>(bytes.len() + 1)?;
                // save the string data
                self.save_bytes(&bytes)?;
                // save null terminator
                self.save(&0u8)?;
            }
            return Ok(());
        }

        Err(io::Error::new(
            io::ErrorKind::InvalidData, 
            format!("encode error with {}: {}", self.encoding.name(), value)
        ))
    }

    /// Save string with automatic encoding detection
    pub fn save_string_auto(&mut self, value: &str) -> io::Result<()> {
        if value.is_empty() {
            // save the string size
            self.save(&1u32)?;
            // save null terminator
            self.save(&0u8)?;
            return Ok(());
        }

        // Determine the best encoding first (without borrowing self)
        let best_encoding = self.determine_best_encoding(value);
        
        // Encode with the determined encoding
        let (bytes, encoding_used) = if let (bytes, _, false) = best_encoding.encode(value) {
            (self.process_encoded_bytes_static(bytes), best_encoding)
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Cannot encode '{}' with any supported encoding", value)
            ));
        };

        // Update encoding if it changed
        if self.encoding != encoding_used {
            self.encoding = encoding_used;
        }
        
        // save the string size
        self.save_as::<usize, u32>(bytes.len() + 1)?;
        // save the string data
        self.save_bytes(&bytes)?;
        // save null terminator
        self.save(&0u8)?;
        Ok(())
    }

    /// Helper method to determine best encoding without borrowing self
    fn determine_best_encoding(&self, text: &str) -> &'static Encoding {
        // Try current encoding first
        if let (_, _, false) = self.encoding.encode(text) {
            return self.encoding;
        }
        
        // Try Windows-1252 (most common)
        if let (_, _, false) = WINDOWS_1252.encode(text) {
            return WINDOWS_1252;
        }
        
        // Check for Cyrillic characters
        if text.chars().any(|c| ('\u{0400}'..='\u{04FF}').contains(&c)) {
            if let (_, _, false) = WINDOWS_1251.encode(text) {
                return WINDOWS_1251;
            }
        }
        
        // Check for Central European characters
        if text.chars().any(|c| {
            matches!(c, 
                'Ą' | 'ą' | 'Ć' | 'ć' | 'Ę' | 'ę' | 'Ł' | 'ł' | 'Ń' | 'ń' | 'Ó' | 'ó' | 
                'Ś' | 'ś' | 'Ź' | 'ź' | 'Ż' | 'ż' | // Polish
                'Č' | 'č' | 'Ď' | 'ď' | 'Ň' | 'ň' | 'Ř' | 'ř' | 'Š' | 'š' | 'Ť' | 'ť' | 
                'Ů' | 'ů' | 'Ž' | 'ž' | // Czech
                'Ľ' | 'ľ' | 'Ŕ' | 'ŕ' | // Slovak
                'Ő' | 'ő' | 'Ű' | 'ű' | // Hungarian
                'Đ' | 'đ' // Croatian/Serbian Latin
            )
        }) {
            if let (_, _, false) = WINDOWS_1250.encode(text) {
                return WINDOWS_1250;
            }
        }
        
        // Try remaining encodings
        if let (_, _, false) = WINDOWS_1250.encode(text) {
            return WINDOWS_1250;
        }
        
        if let (_, _, false) = WINDOWS_1251.encode(text) {
            return WINDOWS_1251;
        }
        
        // Default fallback
        WINDOWS_1252
    }

    /// Helper method to process encoded bytes without lifetime issues
    fn process_encoded_bytes_static(&self, bytes: Cow<[u8]>) -> Vec<u8> {
        match memchr(0, &bytes) {
            None => bytes.into_owned(),
            Some(i) => bytes[..i].to_vec(),
        }
    }

    pub fn save_matrix<S, R, C>(&mut self, matrix: &OMatrix<S, R, C>) -> io::Result<()>
    where
        S: Scalar + Pod,
        R: Dim,
        C: Dim,
        DefaultAllocator: Allocator<S, R, C>,
    {
        self.save_bytes(cast_slice(matrix.as_slice()))
    }

    pub fn encode<'a>(&'a self, str: &'a str) -> io::Result<Cow<'a, [u8]>> {
        if let (bytes, _, false) = self.encoding.encode(str) {
            Ok(self.process_encoded_bytes(bytes))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData, 
                format!("encode error with {}: {}", self.encoding.name(), str)
            ))
        }
    }

    /// Get the name of the current encoding
    pub fn encoding_name(&self) -> &str {
        self.encoding.name()
    }

    /// Check if the current encoding can encode the given string without loss
    pub fn can_encode(&self, text: &str) -> bool {
        let (_, _, had_errors) = self.encoding.encode(text);
        !had_errors
    }
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.cursor.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.cursor.flush()
    }
}
