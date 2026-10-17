//! Built-in procedures and functions beyond input/output and lists: the
//! graphics window, dialog boxes, colors, random numbers, time and the
//! standard library. The checker reads their signatures here; the VM calls
//! `Env::call`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::canvas::parse_color;
use crate::dialogs;
use crate::fonts::Fonts;
use crate::graphics::Graphics;
use crate::library;
use crate::render3d::Vec3;
use crate::tir::BaseType::{self, Bool, Int, Real, Str};
use crate::tir::{join, Type};
use crate::vm::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
    // graphics, dialogs and basics
    Window,
    Open,
    Show,
    Clear,
    Rect,
    Circle,
    Line,
    Text,
    Key,
    MouseX,
    MouseY,
    Click,
    Message,
    Confirm,
    Ask,
    Rgb,
    Random,
    Time,
    Camera,
    Rotate,
    Cube,
    Box3,
    Sphere,
    Triangle3,
    Sin,
    Cos,
    Sqrt,
    Round,
    Font,
    TextWidth,
    // standard library: conversions
    ToInt,
    ToFloat,
    ToText,
    Char,
    Code,
    ToHex,
    ToBinary,
    // math
    Absolute,
    Minimum,
    Maximum,
    Power,
    Floor,
    Ceiling,
    Truncate,
    RoundTo,
    Exponential,
    Logarithm,
    Tan,
    Arcsin,
    Arccos,
    Arctan,
    Arctan2,
    Pi,
    RandomFloat,
    // strings
    Uppercase,
    Lowercase,
    Trim,
    Find,
    Count,
    Replace,
    Split,
    Join,
    StartsWith,
    EndsWith,
    Substring,
    Contains,
    // collections
    Sum,
    IndexOf,
    Sort,
    Reverse,
    Insert,
    TakeLast,
    ClearList,
    // sets
    Add,
    Discard,
    ToList,
    // dictionaries
    HasKey,
    Get,
    RemoveKey,
    Keys,
    Values,
    // files
    ReadFile,
    ReadLines,
    WriteFile,
    AppendFile,
    FileExists,
    // system
    Wait,
    Today,
    Now,
    Stop,
    // module "text" (regular expressions)
    TextMatches,
    TextFind,
    TextReplaceAll,
    TextAll,
    // module "hash" (digests)
    HashSha256,
    HashSha512,
    // module "web" (http requests)
    WebGet,
    // module "data" (JSON)
    DataText,
    DataNumber,
    DataFlag,
    DataHas,
    DataSize,
    DataKeys,
    DataToJson,
    // module "clock" (dates and times)
    ClockStamp,
    ClockFormat,
    ClockParse,
    // module "encode" (base64)
    EncodeBase64,
    EncodeFromBase64,
    // module "shell" (run a system command)
    ShellRun,
    // module "math" (extra maths)
    MathGcd,
    MathLcm,
    MathFactorial,
    MathIsPrime,
    MathSign,
    MathHypot,
    MathClamp,
    MathCbrt,
    // module "bits" (bitwise operations)
    BitsAnd,
    BitsOr,
    BitsXor,
    BitsNot,
    BitsLeft,
    BitsRight,
    BitsOnes,
    // module "base" (number bases)
    BaseTo,
    BaseFrom,
    // module "roman" (Roman numerals)
    RomanTo,
    RomanFrom,
    // module "temp" (temperatures)
    TempCToF,
    TempFToC,
    TempCToK,
    TempKToC,
    // module "angle"
    AngleToRadians,
    AngleToDegrees,
    AngleNormalize,
    // module "percent"
    PercentOf,
    PercentChange,
    // module "geom" (areas and volumes)
    GeomCircleArea,
    GeomRectArea,
    GeomTriangleArea,
    GeomSphereVolume,
    GeomCubeVolume,
    // module "length" (unit conversions)
    LengthMToFt,
    LengthFtToM,
    LengthKmToMi,
    LengthMiToKm,
    // module "weight"
    WeightKgToLb,
    WeightLbToKg,
    // module "size" (byte sizes)
    SizeHuman,
    // module "duration"
    DurationHuman,
    // module "geo" (coordinates)
    GeoDistance,
    // module "ordinal"
    OrdinalOf,
    // module "fib" (Fibonacci)
    FibAt,
    // module "prime"
    PrimeNext,
    PrimeNth,
    // module "str" (more strings)
    StrRepeat,
    StrReverse,
    StrPadLeft,
    StrPadRight,
    StrCenter,
    // module "hex"
    HexEncode,
    HexDecode,
    // module "url"
    UrlEncode,
    UrlDecode,
    // module "case" (naming styles)
    CaseTitle,
    CaseSnake,
    CaseCamel,
    CaseKebab,
    // module "slug"
    SlugMake,
    // module "ascii" (character classes)
    AsciiIsDigit,
    AsciiIsAlpha,
    AsciiIsSpace,
    AsciiIsUpper,
    AsciiIsLower,
    // module "morse"
    MorseEncode,
    MorseDecode,
    // module "caesar" (cipher)
    CaesarEncode,
    CaesarDecode,
    // module "rot13"
    Rot13Apply,
    // module "leet"
    LeetEncode,
    // module "nato" (phonetic alphabet)
    NatoEncode,
    // module "color"
    ColorHex,
    ColorGray,
    ColorInvert,
    // module "valid" (validation)
    ValidEmail,
    ValidInteger,
    ValidNumber,
    // module "luhn"
    LuhnValid,
    // module "isbn"
    IsbnValid,
    // module "uuid"
    UuidMake,
    // module "crc" (checksum)
    CrcValue,
    // module "env" (environment variables)
    EnvGet,
    EnvHas,
    EnvSet,
    // module "path" (file paths)
    PathJoin,
    PathName,
    PathStem,
    PathExtension,
    PathParent,
    PathIsAbsolute,
    // module "os"
    OsName,
    OsArch,
    OsFamily,
    // module "dir" (folders)
    DirList,
    DirMake,
    DirCurrent,
    DirExists,
    // module "file" (file operations)
    FileSize,
    FileCopy,
    FileRename,
    FileModified,
    FileDelete,
    // module "tmp"
    TmpPath,
    // module "args" (program arguments)
    ArgsCount,
    ArgsGet,
    // module "csv"
    CsvFields,
    CsvLine,
    // module "stats"
    StatsMean,
    StatsMedian,
    StatsStdev,
    // module "tax"
    TaxAdd,
    TaxRemove,
    // module "compass"
    CompassDirection,
}

use Builtin::*;

/// How a built-in is typed: a fixed list of parameters, or a rule in
/// `library::check` (for those that accept several types).
enum Sig {
    Fixed(&'static [BaseType], Option<BaseType>),
    Flexible { function: bool },
}

const R: BaseType = Real;
const S: BaseType = Str;
const I: BaseType = Int;

#[rustfmt::skip]
const TABLE: &[(Builtin, &str, Sig)] = &[
    (Window, "window", Sig::Fixed(&[I, I, S], None)),
    (Open, "open", Sig::Fixed(&[], Some(Bool))),
    (Show, "show", Sig::Fixed(&[], None)),
    (Clear, "clear", Sig::Fixed(&[S], None)),
    (Rect, "rect", Sig::Fixed(&[R, R, R, R, S], None)),
    (Circle, "circle", Sig::Fixed(&[R, R, R, S], None)),
    (Line, "line", Sig::Fixed(&[R, R, R, R, S], None)),
    (Text, "text", Sig::Fixed(&[R, R, S, S], None)),
    (Key, "key", Sig::Fixed(&[S], Some(Bool))),
    (MouseX, "mouse_x", Sig::Fixed(&[], Some(Int))),
    (MouseY, "mouse_y", Sig::Fixed(&[], Some(Int))),
    (Click, "click", Sig::Fixed(&[], Some(Bool))),
    (Message, "message", Sig::Fixed(&[S], None)),
    (Confirm, "confirm", Sig::Fixed(&[S], Some(Bool))),
    (Ask, "ask", Sig::Fixed(&[S], Some(Str))),
    (Rgb, "rgb", Sig::Fixed(&[I, I, I], Some(Str))),
    (Random, "random", Sig::Fixed(&[I, I], Some(Int))),
    (Time, "time", Sig::Fixed(&[], Some(Real))),
    (Camera, "camera", Sig::Fixed(&[R, R, R, R, R], None)),
    (Rotate, "rotate", Sig::Fixed(&[R, R, R], None)),
    (Cube, "cube", Sig::Fixed(&[R, R, R, R, S], None)),
    (Box3, "box", Sig::Fixed(&[R, R, R, R, R, R, S], None)),
    (Sphere, "sphere", Sig::Fixed(&[R, R, R, R, S], None)),
    (Triangle3, "triangle3d", Sig::Fixed(&[R, R, R, R, R, R, R, R, R, S], None)),
    (Sin, "sin", Sig::Fixed(&[R], Some(Real))),
    (Cos, "cos", Sig::Fixed(&[R], Some(Real))),
    (Sqrt, "sqrt", Sig::Fixed(&[R], Some(Real))),
    (Round, "round", Sig::Fixed(&[R], Some(Int))),
    (Font, "font", Sig::Fixed(&[S, I], None)),
    (TextWidth, "text_width", Sig::Fixed(&[S], Some(Int))),
    (ToInt, "to_int", Sig::Flexible { function: true }),
    (ToFloat, "to_float", Sig::Flexible { function: true }),
    (ToText, "to_text", Sig::Flexible { function: true }),
    (Char, "char", Sig::Fixed(&[I], Some(Str))),
    (Code, "code", Sig::Fixed(&[S], Some(Int))),
    (ToHex, "to_hex", Sig::Fixed(&[I], Some(Str))),
    (ToBinary, "to_binary", Sig::Fixed(&[I], Some(Str))),
    (Absolute, "absolute", Sig::Flexible { function: true }),
    (Minimum, "minimum", Sig::Flexible { function: true }),
    (Maximum, "maximum", Sig::Flexible { function: true }),
    (Power, "power", Sig::Fixed(&[R, R], Some(Real))),
    (Floor, "floor", Sig::Fixed(&[R], Some(Int))),
    (Ceiling, "ceiling", Sig::Fixed(&[R], Some(Int))),
    (Truncate, "truncate", Sig::Fixed(&[R], Some(Int))),
    (RoundTo, "round_to", Sig::Fixed(&[R, I], Some(Real))),
    (Exponential, "exponential", Sig::Fixed(&[R], Some(Real))),
    (Logarithm, "logarithm", Sig::Flexible { function: true }),
    (Tan, "tan", Sig::Fixed(&[R], Some(Real))),
    (Arcsin, "arcsin", Sig::Fixed(&[R], Some(Real))),
    (Arccos, "arccos", Sig::Fixed(&[R], Some(Real))),
    (Arctan, "arctan", Sig::Fixed(&[R], Some(Real))),
    (Arctan2, "arctan2", Sig::Fixed(&[R, R], Some(Real))),
    (Pi, "pi", Sig::Fixed(&[], Some(Real))),
    (RandomFloat, "random_float", Sig::Fixed(&[], Some(Real))),
    (Uppercase, "uppercase", Sig::Fixed(&[S], Some(Str))),
    (Lowercase, "lowercase", Sig::Fixed(&[S], Some(Str))),
    (Trim, "trim", Sig::Fixed(&[S], Some(Str))),
    (Find, "find", Sig::Fixed(&[S, S], Some(Int))),
    (Count, "count", Sig::Fixed(&[S, S], Some(Int))),
    (Replace, "replace", Sig::Fixed(&[S, S, S], Some(Str))),
    (Split, "split", Sig::Flexible { function: true }),
    (Join, "join", Sig::Flexible { function: true }),
    (StartsWith, "starts_with", Sig::Fixed(&[S, S], Some(Bool))),
    (EndsWith, "ends_with", Sig::Fixed(&[S, S], Some(Bool))),
    (Substring, "substring", Sig::Fixed(&[S, I, I], Some(Str))),
    (Contains, "contains", Sig::Flexible { function: true }),
    (Sum, "sum", Sig::Flexible { function: true }),
    (IndexOf, "index_of", Sig::Flexible { function: true }),
    (Sort, "sort", Sig::Flexible { function: false }),
    (Reverse, "reverse", Sig::Flexible { function: false }),
    (Insert, "insert", Sig::Flexible { function: false }),
    (TakeLast, "take_last", Sig::Flexible { function: true }),
    (ClearList, "clear_list", Sig::Flexible { function: false }),
    (Add, "add", Sig::Flexible { function: false }),
    (Discard, "discard", Sig::Flexible { function: false }),
    (ToList, "to_list", Sig::Flexible { function: true }),
    (HasKey, "has_key", Sig::Flexible { function: true }),
    (Get, "get", Sig::Flexible { function: true }),
    (RemoveKey, "remove_key", Sig::Flexible { function: false }),
    (Keys, "keys", Sig::Flexible { function: true }),
    (Values, "values", Sig::Flexible { function: true }),
    (ReadFile, "read_file", Sig::Fixed(&[S], Some(Str))),
    (ReadLines, "read_lines", Sig::Flexible { function: true }),
    (WriteFile, "write_file", Sig::Fixed(&[S, S], None)),
    (AppendFile, "append_file", Sig::Fixed(&[S, S], None)),
    (FileExists, "file_exists", Sig::Fixed(&[S], Some(Bool))),
    (Wait, "wait", Sig::Fixed(&[I], None)),
    (Today, "today", Sig::Fixed(&[], Some(Str))),
    (Now, "now", Sig::Fixed(&[], Some(Str))),
    (Stop, "stop", Sig::Fixed(&[], None)),
    (TextMatches, "text.matches", Sig::Fixed(&[S, S], Some(Bool))),
    (TextFind, "text.find", Sig::Fixed(&[S, S], Some(Str))),
    (TextReplaceAll, "text.replace_all", Sig::Fixed(&[S, S, S], Some(Str))),
    (TextAll, "text.all", Sig::Flexible { function: true }),
    (HashSha256, "hash.sha256", Sig::Fixed(&[S], Some(Str))),
    (HashSha512, "hash.sha512", Sig::Fixed(&[S], Some(Str))),
    (WebGet, "web.get", Sig::Fixed(&[S], Some(Str))),
    (DataText, "data.text", Sig::Fixed(&[S, S], Some(Str))),
    (DataNumber, "data.number", Sig::Fixed(&[S, S], Some(Real))),
    (DataFlag, "data.flag", Sig::Fixed(&[S, S], Some(Bool))),
    (DataHas, "data.has", Sig::Fixed(&[S, S], Some(Bool))),
    (DataSize, "data.size", Sig::Fixed(&[S, S], Some(Int))),
    (DataKeys, "data.keys", Sig::Flexible { function: true }),
    (DataToJson, "data.to_json", Sig::Flexible { function: true }),
    (ClockStamp, "clock.stamp", Sig::Fixed(&[], Some(Int))),
    (ClockFormat, "clock.format", Sig::Fixed(&[I, S], Some(Str))),
    (ClockParse, "clock.parse", Sig::Fixed(&[S, S], Some(Int))),
    (EncodeBase64, "encode.base64", Sig::Fixed(&[S], Some(Str))),
    (EncodeFromBase64, "encode.from_base64", Sig::Fixed(&[S], Some(Str))),
    (ShellRun, "shell.run", Sig::Fixed(&[S], Some(Str))),
    (MathGcd, "math.gcd", Sig::Fixed(&[I, I], Some(Int))),
    (MathLcm, "math.lcm", Sig::Fixed(&[I, I], Some(Int))),
    (MathFactorial, "math.factorial", Sig::Fixed(&[I], Some(Int))),
    (MathIsPrime, "math.is_prime", Sig::Fixed(&[I], Some(Bool))),
    (MathSign, "math.sign", Sig::Fixed(&[R], Some(Int))),
    (MathHypot, "math.hypot", Sig::Fixed(&[R, R], Some(Real))),
    (MathClamp, "math.clamp", Sig::Fixed(&[R, R, R], Some(Real))),
    (MathCbrt, "math.cbrt", Sig::Fixed(&[R], Some(Real))),
    (BitsAnd, "bits.and", Sig::Fixed(&[I, I], Some(Int))),
    (BitsOr, "bits.or", Sig::Fixed(&[I, I], Some(Int))),
    (BitsXor, "bits.xor", Sig::Fixed(&[I, I], Some(Int))),
    (BitsNot, "bits.not", Sig::Fixed(&[I], Some(Int))),
    (BitsLeft, "bits.left", Sig::Fixed(&[I, I], Some(Int))),
    (BitsRight, "bits.right", Sig::Fixed(&[I, I], Some(Int))),
    (BitsOnes, "bits.ones", Sig::Fixed(&[I], Some(Int))),
    (BaseTo, "base.to", Sig::Fixed(&[I, I], Some(Str))),
    (BaseFrom, "base.from", Sig::Fixed(&[S, I], Some(Int))),
    (RomanTo, "roman.to", Sig::Fixed(&[I], Some(Str))),
    (RomanFrom, "roman.from", Sig::Fixed(&[S], Some(Int))),
    (TempCToF, "temp.c_to_f", Sig::Fixed(&[R], Some(Real))),
    (TempFToC, "temp.f_to_c", Sig::Fixed(&[R], Some(Real))),
    (TempCToK, "temp.c_to_k", Sig::Fixed(&[R], Some(Real))),
    (TempKToC, "temp.k_to_c", Sig::Fixed(&[R], Some(Real))),
    (AngleToRadians, "angle.to_radians", Sig::Fixed(&[R], Some(Real))),
    (AngleToDegrees, "angle.to_degrees", Sig::Fixed(&[R], Some(Real))),
    (AngleNormalize, "angle.normalize", Sig::Fixed(&[R], Some(Real))),
    (PercentOf, "percent.of", Sig::Fixed(&[R, R], Some(Real))),
    (PercentChange, "percent.change", Sig::Fixed(&[R, R], Some(Real))),
    (GeomCircleArea, "geom.circle_area", Sig::Fixed(&[R], Some(Real))),
    (GeomRectArea, "geom.rect_area", Sig::Fixed(&[R, R], Some(Real))),
    (GeomTriangleArea, "geom.triangle_area", Sig::Fixed(&[R, R], Some(Real))),
    (GeomSphereVolume, "geom.sphere_volume", Sig::Fixed(&[R], Some(Real))),
    (GeomCubeVolume, "geom.cube_volume", Sig::Fixed(&[R], Some(Real))),
    (LengthMToFt, "length.meters_to_feet", Sig::Fixed(&[R], Some(Real))),
    (LengthFtToM, "length.feet_to_meters", Sig::Fixed(&[R], Some(Real))),
    (LengthKmToMi, "length.km_to_miles", Sig::Fixed(&[R], Some(Real))),
    (LengthMiToKm, "length.miles_to_km", Sig::Fixed(&[R], Some(Real))),
    (WeightKgToLb, "weight.kg_to_lb", Sig::Fixed(&[R], Some(Real))),
    (WeightLbToKg, "weight.lb_to_kg", Sig::Fixed(&[R], Some(Real))),
    (SizeHuman, "size.human", Sig::Fixed(&[I], Some(Str))),
    (DurationHuman, "duration.human", Sig::Fixed(&[I], Some(Str))),
    (GeoDistance, "geo.distance", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (OrdinalOf, "ordinal.of", Sig::Fixed(&[I], Some(Str))),
    (FibAt, "fib.at", Sig::Fixed(&[I], Some(Int))),
    (PrimeNext, "prime.next", Sig::Fixed(&[I], Some(Int))),
    (PrimeNth, "prime.nth", Sig::Fixed(&[I], Some(Int))),
    (StrRepeat, "str.repeat", Sig::Fixed(&[S, I], Some(Str))),
    (StrReverse, "str.reverse", Sig::Fixed(&[S], Some(Str))),
    (StrPadLeft, "str.pad_left", Sig::Fixed(&[S, I, S], Some(Str))),
    (StrPadRight, "str.pad_right", Sig::Fixed(&[S, I, S], Some(Str))),
    (StrCenter, "str.center", Sig::Fixed(&[S, I, S], Some(Str))),
    (HexEncode, "hex.encode", Sig::Fixed(&[S], Some(Str))),
    (HexDecode, "hex.decode", Sig::Fixed(&[S], Some(Str))),
    (UrlEncode, "url.encode", Sig::Fixed(&[S], Some(Str))),
    (UrlDecode, "url.decode", Sig::Fixed(&[S], Some(Str))),
    (CaseTitle, "case.title", Sig::Fixed(&[S], Some(Str))),
    (CaseSnake, "case.snake", Sig::Fixed(&[S], Some(Str))),
    (CaseCamel, "case.camel", Sig::Fixed(&[S], Some(Str))),
    (CaseKebab, "case.kebab", Sig::Fixed(&[S], Some(Str))),
    (SlugMake, "slug.make", Sig::Fixed(&[S], Some(Str))),
    (AsciiIsDigit, "ascii.is_digit", Sig::Fixed(&[S], Some(Bool))),
    (AsciiIsAlpha, "ascii.is_alpha", Sig::Fixed(&[S], Some(Bool))),
    (AsciiIsSpace, "ascii.is_space", Sig::Fixed(&[S], Some(Bool))),
    (AsciiIsUpper, "ascii.is_upper", Sig::Fixed(&[S], Some(Bool))),
    (AsciiIsLower, "ascii.is_lower", Sig::Fixed(&[S], Some(Bool))),
    (MorseEncode, "morse.encode", Sig::Fixed(&[S], Some(Str))),
    (MorseDecode, "morse.decode", Sig::Fixed(&[S], Some(Str))),
    (CaesarEncode, "caesar.encode", Sig::Fixed(&[S, I], Some(Str))),
    (CaesarDecode, "caesar.decode", Sig::Fixed(&[S, I], Some(Str))),
    (Rot13Apply, "rot13.apply", Sig::Fixed(&[S], Some(Str))),
    (LeetEncode, "leet.encode", Sig::Fixed(&[S], Some(Str))),
    (NatoEncode, "nato.encode", Sig::Fixed(&[S], Some(Str))),
    (ColorHex, "color.hex", Sig::Fixed(&[I, I, I], Some(Str))),
    (ColorGray, "color.gray", Sig::Fixed(&[I], Some(Str))),
    (ColorInvert, "color.invert", Sig::Fixed(&[S], Some(Str))),
    (ValidEmail, "valid.email", Sig::Fixed(&[S], Some(Bool))),
    (ValidInteger, "valid.integer", Sig::Fixed(&[S], Some(Bool))),
    (ValidNumber, "valid.number", Sig::Fixed(&[S], Some(Bool))),
    (LuhnValid, "luhn.valid", Sig::Fixed(&[S], Some(Bool))),
    (IsbnValid, "isbn.valid", Sig::Fixed(&[S], Some(Bool))),
    (UuidMake, "uuid.make", Sig::Fixed(&[], Some(Str))),
    (CrcValue, "crc.value", Sig::Fixed(&[S], Some(Int))),
    (EnvGet, "env.get", Sig::Fixed(&[S], Some(Str))),
    (EnvHas, "env.has", Sig::Fixed(&[S], Some(Bool))),
    (EnvSet, "env.set", Sig::Fixed(&[S, S], None)),
    (PathJoin, "path.join", Sig::Fixed(&[S, S], Some(Str))),
    (PathName, "path.name", Sig::Fixed(&[S], Some(Str))),
    (PathStem, "path.stem", Sig::Fixed(&[S], Some(Str))),
    (PathExtension, "path.extension", Sig::Fixed(&[S], Some(Str))),
    (PathParent, "path.parent", Sig::Fixed(&[S], Some(Str))),
    (PathIsAbsolute, "path.is_absolute", Sig::Fixed(&[S], Some(Bool))),
    (OsName, "os.name", Sig::Fixed(&[], Some(Str))),
    (OsArch, "os.arch", Sig::Fixed(&[], Some(Str))),
    (OsFamily, "os.family", Sig::Fixed(&[], Some(Str))),
    (DirList, "dir.list", Sig::Flexible { function: true }),
    (DirMake, "dir.make", Sig::Fixed(&[S], None)),
    (DirCurrent, "dir.current", Sig::Fixed(&[], Some(Str))),
    (DirExists, "dir.exists", Sig::Fixed(&[S], Some(Bool))),
    (FileSize, "file.size", Sig::Fixed(&[S], Some(Int))),
    (FileCopy, "file.copy", Sig::Fixed(&[S, S], None)),
    (FileRename, "file.rename", Sig::Fixed(&[S, S], None)),
    (FileModified, "file.modified", Sig::Fixed(&[S], Some(Int))),
    (FileDelete, "file.delete", Sig::Fixed(&[S], None)),
    (TmpPath, "tmp.path", Sig::Fixed(&[], Some(Str))),
    (ArgsCount, "args.count", Sig::Fixed(&[], Some(Int))),
    (ArgsGet, "args.get", Sig::Fixed(&[I], Some(Str))),
    (CsvFields, "csv.fields", Sig::Flexible { function: true }),
    (CsvLine, "csv.line", Sig::Flexible { function: true }),
    (StatsMean, "stats.mean", Sig::Flexible { function: true }),
    (StatsMedian, "stats.median", Sig::Flexible { function: true }),
    (StatsStdev, "stats.stdev", Sig::Flexible { function: true }),
    (TaxAdd, "tax.add", Sig::Fixed(&[R, R], Some(Real))),
    (TaxRemove, "tax.remove", Sig::Fixed(&[R, R], Some(Real))),
    (CompassDirection, "compass.direction", Sig::Fixed(&[R], Some(Str))),
];

/// Built-in modules provided by the interpreter (imported with `import <name>`).
pub const BUILTIN_MODULES: &[&str] = &[
    "text", "hash", "web", "data", "clock", "encode", "shell", "math", "bits", "base", "roman", "temp", "angle",
    "percent", "geom", "length", "weight", "size", "duration", "geo", "ordinal", "fib", "prime", "str", "hex", "url",
    "case", "slug", "ascii", "morse", "caesar", "rot13", "leet", "nato", "color", "valid", "luhn", "isbn", "uuid",
    "crc", "env", "path", "os", "dir", "file", "tmp", "args", "csv", "stats", "tax", "compass",
];

pub fn is_builtin_module(name: &str) -> bool {
    BUILTIN_MODULES.contains(&name)
}

/// Why a call does not fit a built-in's signature.
pub enum CheckError {
    /// the built-in takes this many values
    Arity(usize),
    /// argument `index` (0-based) should have this type
    Arg { index: usize, expected: Type },
    /// any other rule, as a complete message
    Other(String),
}

/// A call that fits: how to convert each argument (int → float), and the result type.
pub struct Checked {
    pub convert: Vec<Option<BaseType>>,
    pub ret: Type,
}

impl Builtin {
    fn entry(self) -> &'static (Builtin, &'static str, Sig) {
        TABLE.iter().find(|(b, _, _)| *b == self).expect("every builtin is in the table")
    }

    pub fn name(self) -> &'static str {
        self.entry().1
    }

    /// Functions return a value and are used in expressions; procedures are statements.
    pub fn is_function(self) -> bool {
        match self.entry().2 {
            Sig::Fixed(_, ret) => ret.is_some(),
            Sig::Flexible { function } => function,
        }
    }

    /// Checks the argument types of a call.
    pub fn check(self, args: &[Type]) -> Result<Checked, CheckError> {
        match &self.entry().2 {
            Sig::Fixed(params, ret) => {
                if args.len() != params.len() {
                    return Err(CheckError::Arity(params.len()));
                }
                for (index, (&a, &p)) in args.iter().zip(params.iter()).enumerate() {
                    let fits = match a {
                        Type::Unknown => true,
                        Type::Base(x) => join(p, x) == Some(p),
                        _ => false,
                    };
                    if !fits {
                        return Err(CheckError::Arg { index, expected: Type::Base(p) });
                    }
                }
                Ok(Checked {
                    convert: params.iter().map(|&p| Some(p)).collect(),
                    ret: ret.map_or(Type::Unknown, Type::Base),
                })
            }
            Sig::Flexible { .. } => library::check(self, args),
        }
    }

    pub fn id(self) -> u16 {
        TABLE.iter().position(|(b, _, _)| *b == self).expect("every builtin is in the table") as u16
    }

    pub fn from_id(id: u16) -> Builtin {
        TABLE[id as usize].0
    }
}

pub fn lookup(name: &str) -> Option<Builtin> {
    TABLE.iter().find(|(_, n, _)| *n == name).map(|(b, _, _)| *b)
}

const NO_WINDOW: &str = "there is no window: call window(...) first";

fn int(v: &Value) -> i64 {
    match v {
        Value::Int(n) => *n,
        other => panic!("internal error: expected an int, found {:?}", other),
    }
}

fn real(v: &Value) -> f64 {
    match v {
        Value::Real(r) => *r,
        other => panic!("internal error: expected a float, found {:?}", other),
    }
}

fn text(v: &Value) -> &str {
    match v {
        Value::Str(s) => s,
        other => panic!("internal error: expected a string, found {:?}", other),
    }
}

/// Run-time state of the built-ins: the window, the clock and the random generator.
pub struct Env {
    graphics: Option<Graphics>,
    fonts: Fonts,
    start: Instant,
    rng: u64,
}

impl Default for Env {
    fn default() -> Env {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        Env { graphics: None, fonts: Fonts::default(), start: Instant::now(), rng: seed | 1 }
    }
}

impl Env {
    fn next_random(&mut self) -> u64 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    fn window(&mut self) -> Result<&mut Graphics, String> {
        self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())
    }

    /// Runs a built-in; procedures return `Value::Null`.
    pub fn call(&mut self, b: Builtin, args: &[Value]) -> Result<Value, String> {
        Ok(match b {
            Rgb => {
                let c = |v: &Value| int(v).clamp(0, 255);
                Value::Str(format!("#{:02x}{:02x}{:02x}", c(&args[0]), c(&args[1]), c(&args[2])).into())
            }
            Random => {
                let (lo, hi) = (int(&args[0]), int(&args[1]));
                if lo > hi {
                    return Err(format!("random: the minimum ({}) is greater than the maximum ({})", lo, hi));
                }
                let span = (hi as i128 - lo as i128 + 1) as u128;
                Value::Int((lo as i128 + (self.next_random() as u128 % span) as i128) as i64)
            }
            Time => Value::Real(self.start.elapsed().as_secs_f64()),
            Sin => Value::Real(real(&args[0]).to_radians().sin()),
            Cos => Value::Real(real(&args[0]).to_radians().cos()),
            Sqrt => {
                let x = real(&args[0]);
                if x < 0.0 {
                    return Err(format!("sqrt of a negative number ({})", crate::vm::format_real(x)));
                }
                Value::Real(x.sqrt())
            }
            Round => {
                let r = real(&args[0]).round();
                if !(-9.2e18..=9.2e18).contains(&r) {
                    return Err("number too large: the result exceeds the integer limits".to_string());
                }
                Value::Int(r as i64)
            }
            Window => {
                let (w, h) = (int(&args[0]), int(&args[1]));
                if w <= 0 || h <= 0 || w > 10_000 || h > 10_000 {
                    return Err(format!("the window size must be between 1 and 10000, here it is {} x {}", w, h));
                }
                self.graphics = None; // close the previous window first
                self.graphics = Some(Graphics::new(w as usize, h as usize, text(&args[2]))?);
                Value::Null
            }
            Open => Value::Bool(self.graphics.as_ref().is_some_and(|g| g.is_open())),
            Show => {
                self.window()?.show();
                Value::Null
            }
            Clear => {
                let c = parse_color(text(&args[0]))?;
                self.window()?.canvas.clear(c);
                Value::Null
            }
            Rect => {
                let c = parse_color(text(&args[4]))?;
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                self.window()?.canvas.rect(x, y, w, h, c);
                Value::Null
            }
            Circle => {
                let c = parse_color(text(&args[3]))?;
                let (x, y, r) = (real(&args[0]), real(&args[1]), real(&args[2]));
                self.window()?.canvas.circle(x, y, r, c);
                Value::Null
            }
            Line => {
                let c = parse_color(text(&args[4]))?;
                let (x1, y1, x2, y2) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                self.window()?.canvas.line(x1, y1, x2, y2, c);
                Value::Null
            }
            Text => {
                let c = parse_color(text(&args[3]))?;
                let (x, y) = (real(&args[0]), real(&args[1]));
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                self.fonts.draw(&mut g.canvas, x, y, text(&args[2]), c)?;
                Value::Null
            }
            Font => {
                self.fonts.set(text(&args[0]), int(&args[1]))?;
                Value::Null
            }
            TextWidth => Value::Int(self.fonts.width(text(&args[0]))?),
            RandomFloat => Value::Real((self.next_random() >> 11) as f64 / (1u64 << 53) as f64),
            Stop => Value::Null, // the VM ends the program after this call
            Key => Value::Bool(self.window()?.key(text(&args[0]))?),
            MouseX => Value::Int(self.window()?.mouse().0),
            MouseY => Value::Int(self.window()?.mouse().1),
            Click => Value::Bool(self.window()?.click()),
            Message => {
                dialogs::message(text(&args[0]))?;
                Value::Null
            }
            Confirm => Value::Bool(dialogs::confirm(text(&args[0]))?),
            Ask => Value::Str(dialogs::ask(text(&args[0]))?.into()),
            Camera => {
                let s = &mut self.window()?.scene;
                s.cam = Vec3::new(real(&args[0]), real(&args[1]), real(&args[2]));
                s.turn = real(&args[3]);
                s.tilt = real(&args[4]);
                Value::Null
            }
            Rotate => {
                self.window()?.scene.rot = (real(&args[0]), real(&args[1]), real(&args[2]));
                Value::Null
            }
            Cube | Sphere => {
                let c = parse_color(text(&args[4]))?;
                let center = Vec3::new(real(&args[0]), real(&args[1]), real(&args[2]));
                let g = self.window()?;
                if b == Cube {
                    g.scene.cube(&mut g.canvas, center, real(&args[3]), c);
                } else {
                    g.scene.sphere(&mut g.canvas, center, real(&args[3]), c);
                }
                Value::Null
            }
            Box3 => {
                let c = parse_color(text(&args[6]))?;
                let center = Vec3::new(real(&args[0]), real(&args[1]), real(&args[2]));
                let size = Vec3::new(real(&args[3]), real(&args[4]), real(&args[5]));
                let g = self.window()?;
                g.scene.box_(&mut g.canvas, center, size, c);
                Value::Null
            }
            Triangle3 => {
                let c = parse_color(text(&args[9]))?;
                let p = |k: usize| Vec3::new(real(&args[k]), real(&args[k + 1]), real(&args[k + 2]));
                let g = self.window()?;
                g.scene.triangle(&mut g.canvas, p(0), p(3), p(6), c, true);
                Value::Null
            }
            _ => library::call(b, args)?,
        })
    }

    /// Closes the window, if any (used by `stop`).
    pub fn close_window(&mut self) {
        self.graphics = None;
    }

    /// At the end of the program an open window keeps showing its last frame
    /// until the user closes it.
    pub fn finish(&mut self) {
        if let Some(g) = self.graphics.as_mut() {
            g.keep_open();
        }
    }
}
