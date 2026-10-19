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

    // ---- batch A: unit conversions ----
    AreaSqmToSqft, AreaSqftToSqm, AreaAcreToSqm, AreaSqmToAcre, AreaHectareToAcre, AreaAcreToHectare,
    VolLToGal, VolGalToL, VolLToMl, VolMlToL, VolCupToMl, VolMlToCup,
    SpeedKmhToMph, SpeedMphToKmh, SpeedMpsToKmh, SpeedKmhToMps, SpeedKnotToKmh, SpeedKmhToKnot,
    PaToBar, BarToPa, PsiToBar, BarToPsi, AtmToBar, BarToAtm,
    JouleToCal, CalToJoule, KwhToJoule, JouleToKwh,
    WattToHp, HpToWatt,
    MpgToL100, L100ToMpg,
    NewtonToLbf, LbfToNewton,
    NmToLbft, LbftToNm,
    MbpsToMbs, MbsToMbps,
    HzToRpm, RpmToHz,
    TbspToMl, MlToTbsp, TspToMl, MlToTsp,
    NmiToKm, KmToNmi,
    AuToKm, KmToAu, LyToKm, KmToLy,
    StoneToKg, KgToStone,
    Gcm3ToKgm3, Kgm3ToGcm3,
    GainToDb, DbToGain,
    BpmToMs, MsToBpm,
    DegToGrad, GradToDeg, RadToGrad, GradToRad,
    HoursToSeconds, SecondsToHours, DaysToSeconds, SecondsToDays,
    MinkmToKmh, KmhToMinkm,

    // ---- batch B: maths ----
    HyperSinh, HyperCosh, HyperTanh,
    AhyperAsinh, AhyperAcosh, AhyperAtanh,
    LogsTwo, LogsTen, LogsNatural,
    TrigxSec, TrigxCsc, TrigxCot,
    ChooseCombinations, ChoosePermutations,
    SeqTriangular, SeqSquare, SeqCube, SeqCatalan, SeqTetrahedral,
    ModularPow,
    LerpBetween, LerpMapRange,
    RoundingUp, RoundingDown, RoundingHalfUp, RoundingToMultiple,
    Vec2Length, Vec2Dot, Vec2Angle,
    Vec3Length, Vec3Dot,
    ComplexAbs, ComplexArg,
    ParityIsEven, ParityIsOdd,
    DivisibleBy,
    DigitsSum, DigitsCount, DigitsReverse,
    CollatzSteps,
    PerfectIs,
    ArmstrongIs,
    PalindromeText, PalindromeNumber,
    RangeContains,
    PointDistance, PointManhattan,
    SlopeBetween,
    RootSquare, RootCube, RootNth,

    // ---- batch C: more text ----
    CountWords, CountLines, CountChars, CountVowels,
    TruncateTo, TruncateWords,
    IndentBy,
    StripPrefix, StripSuffix,
    MaskAll, MaskMiddle,
    InitialsOf,
    AcronymOf,
    PigLatin,
    AtbashApply,
    VigenereEncode, VigenereDecode,
    Rot47Apply,
    BinEncode, BinDecode,
    Base32Encode, Base32Decode,
    HtmlEscape, HtmlUnescape,
    ShoutIt, ShoutWhisper,
    WordsReverse, WordsFirst, WordsLast,
    UnicodeCodepoint, UnicodeFromCodepoint, UnicodeSize,
    NormalizeSpaces,
    AnagramOf,
    FizzbuzzAt,
    QuoteAdd,
    RleEncode, RleDecode,
    WrapAt,
    SpongebobMock,
    ClapText,

    // ---- batch D: science, finance, geometry, dates ----
    BmiValue, BmiCategory,
    OhmVoltage, OhmCurrent, OhmResistance, OhmPower,
    PhysicsForce, PhysicsKinetic, PhysicsMomentum,
    WaveFrequency, WaveWavelength, WavePeriod,
    GravityOn,
    InterestSimple, InterestCompound,
    LoanPayment,
    TipAmount, TipTotal,
    DiscountApply, DiscountSaved,
    RoiPercent,
    MoneyFormat,
    BearingBetween,
    TriangleHypotenuse, TriangleHeron, TriangleIsRight,
    CircleArea, CircleCircumference, CircleDiameter,
    RectangleArea, RectanglePerimeter, RectangleDiagonal,
    SphereVolume, SphereSurface,
    CylinderVolume, CylinderSurface,
    ConeVolume, ConeSurface,
    WeekdayOf,
    MonthName, MonthDays,
    LeapYear,
    AgeYears,
    ZodiacSign,
    SeasonOf,
    GradeLetter,
    LogicXor, LogicImplies, LogicNand, LogicNor,
    YesnoOf,
    ScientificNotation,
    FractionToDecimal, FractionSimplify,
    HeartrateMax, HeartrateZone,

    // ---- batch E: hashing, validation, randomness, ids, utilities ----
    FnvHash, Djb2Hash, SdbmHash, AdlerValue,
    Ipv4Valid, MacValid, HexcolorValid, WeburlValid,
    DiceRoll, DiceMany,
    CoinFlip,
    PickBetween, PickChance,
    TokenHex,
    PasswordMake,
    NanoidMake,
    OtpCode,
    LoremWords,
    RandcolorHex,
    FactorCount, FactorSum,
    TotientOf,
    CompareOf,
    ClampintBetween,
    PowSquare, PowCube, PowOf,
    CapsFirst,
    SwapcaseApply,
    ConsonantsCount,
    Point3Distance,
    MidpointOf,
    IsqrtOf,
    CycleIndex,
    DigitrootOf,

    // ---- batch F: advanced geometry, finance, weather, physics ----
    TrapezoidArea,
    EllipseArea, EllipsePerimeter,
    PentagonArea,
    HexagonArea,
    PolygonInteriorAngle, PolygonArea,
    PyramidVolume,
    PrismVolume,
    TorusVolume,
    CuboidVolume, CuboidSurface,
    AnnulusArea,
    SavingsFutureValue,
    DepreciationStraightLine,
    CentsToCents, CentsFromCents,
    MarkupPrice, MarkupMargin,
    BmrMen, BmrWomen,
    WindChill,
    HeatIndex,
    DewpointOf,
    QuadraticDiscriminant, QuadraticRoot1, QuadraticRoot2,
    MotionSpeed, MotionTime, MotionDistance,
    AprToMonthly, AprToAnnual,

    // ---- module "grid": fast contiguous float arrays (the array engine) ----
    GridRange, GridZeros, GridFill, GridOf,
    GridSize, GridGet, GridSet, GridFree, GridToList,
    GridAdd, GridSub, GridMul, GridDiv, GridScale, GridOffset,
    GridSubScalar, GridScalarSub, GridDivScalar, GridScalarDiv,
    GridSqrt, GridSquare, GridAbs,
    GridSum, GridMean, GridMax, GridMin, GridDot, GridSumSquares, GridNorm,
    /// the fused element-wise kernel (compiler-generated from a grid expression)
    GridEval,
    GridMatmul,
    // module "counter" (tallies)
    CounterItems,
    CounterChars,
    CounterWords,
    // module "web" (more requests)
    WebPost,
    WebStatus,
    WebDownload,
    // module "data" (more JSON)
    DataPretty,
    DataType,
    // module "text" (more regex)
    TextSplit,
    TextCount,
    TextGroup,
    // module "chart" (plots to SVG files, like matplotlib savefig)
    ChartLine,
    ChartBars,
    ChartScatter,
    // module "table" (CSV columns and aggregates, like a light pandas)
    TableHeaders,
    TableColumn,
    TableNumbers,
    TableRows,
    TableMean,
    TableSum,
    // module "iter" (combinatorics over a string's characters)
    IterCombinations,
    IterPermutations,
    IterProduct,
    // module "pick" (random choices over lists)
    PickShuffle,
    PickChoice,
    PickSample,
    // module "clock" (timestamp arithmetic)
    ClockAdd,
    ClockDiff,

    // ======== batch G1: more conversions, maths, strings ========
    AccelGToMps2, AccelMps2ToG,
    LuxFcToLux, LuxLuxToFc,
    RadGrayToRad, RadRadToGray, RadSvToRem, RadRemToSv,
    TurnToDeg, TurnFromDeg, TurnToRad, TurnFromRad,
    FlowLpmToM3h, FlowM3hToLpm,
    TypoPtToMm, TypoMmToPt, TypoPtToInch, TypoInchToPt,
    OvenCToGas, OvenGasToC,
    GlucMgdlToMmol, GlucMmolToMgdl,
    ScreenPpi, AspectRatio,
    BeaufortFromKmh,
    QuakeEnergy,
    WrapvInto,
    SmoothstepOf,
    StepAt,
    SincOf,
    SigmoidOf,
    ReluOf,
    SoftsignOf,
    GaussPdf,
    ErfOf,
    GammafOf,
    HarmonicNumber,
    GeoseriesSum,
    ArithseriesSum,
    ModuloWrap, ModuloFloordiv,
    Hypot3Of,
    ClampfBetween,
    FracPart, FracWhole,
    ReciprocalOf,
    PreciseLog1p, PreciseExpm1,
    RotstrLeft, RotstrRight,
    ZfillOf,
    NospacesOf,
    DigitsonlyOf,
    LettersonlyOf,
    DisemvowelOf,
    ScreamingOf,
    DotcaseOf,
    PathcaseOf,
    SentencecaseOf,
    DedupChars,
    ExpandtabsOf,
    Rot5Digits,
    HammingDistance,
    LevenshteinDistance,
    SoundexOf,
    SyllablesCount,
    AvgwordAverage,
    ReadtimeMinutes,

    // ======== batch G2: physics, finance, health, geometry ========
    KinVfinal, KinDistance,
    ProjRange, ProjMaxHeight, ProjFlightTime,
    PendPeriod,
    SpringPeriod, SpringForce,
    LensFocal, LensDiopter,
    ResSeries, ResParallel,
    CapSeries, CapParallel,
    BuoyForce,
    SpecGravity,
    ThermoEnergy,
    EfficiencyPercent,
    WorkJoules,
    MomentOf,
    ImpulseOf,
    DopplerObserved,
    EscapeVelocity,
    OrbitPeriod,
    RedshiftZ,
    LorentzFactor,
    HalflifeRemaining,
    DecayAmount,
    GrowthAmount,
    Rule72Years,
    AnnuityPv,
    BreakevenUnits,
    ProfitAmount, ProfitMargin,
    CommissionOf,
    WageWeekly, WageOvertime,
    InflationAdjust,
    IdealweightForHeight,
    TdeeFromBmr,
    WaterDaily,
    DosageByWeight,
    GforceTurn,
    GearRatio,
    AutonomyDistance,
    EmissionsFromKm,
    SolarWatts,
    BatteryRuntime,
    LedResistor,
    FreefallVelocity, FreefallDistance,
    TetraVolume, TetraSurface,
    OctaVolume,
    ParallelogramArea,
    RhombusArea,
    SectorArea, SectorArc,
    EllipsoidVolume,
    NgonPerimeter, NgonApothem,
    FrustumVolume,
    InclinePctToDeg, InclineDegToPct,

    // ======== batch G3: sequences, checks, formatting, misc ========
    ApproxEqual,
    ProportionSolve,
    MeansOf2, MeansGeometric2, MeansHarmonic2,
    BankersRound,
    PluralOf,
    AbbreviateNumber,
    GpaFromPercent,
    CurveGrade,
    ZscoreOf,
    MinmaxnormOf,
    Clamp01Of,
    CerpOf,
    NibbleHigh, NibbleLow,
    ByteopsHigh, ByteopsLow,
    EndianSwap16, EndianSwap32,
    Nextpow2Of,
    Ispow2Of,
    AlignUp, AlignDown,
    GraycodeEncode, GraycodeDecode,
    EanCheckDigit,
    CardtypeOf,
    HumidexOf,
    PhFromConcentration, PhConcentration,
    SoundAdd,
    NoteFrequency, NoteCents,
    ProbAnd, ProbOr, ProbInvert,
    OddsToProb, OddsFromProb,
    CollatzmaxPeak,
    HappyIs,
    AliquotIsAbundant, AliquotIsDeficient, AliquotIsPerfect,
    PolygonalNumber,
    PronicNumber,
    PentagonalNumber,
    HexagonalNumber,
    StarNumber,
    LucasNumber,
    TribonacciNumber,
    PadovanNumber,
    PrimorialOf,
    PrimecountBelow,
    PrevprimeBefore,
    CoprimeAre,
    AmicableAre,
    MultiGcd3, MultiLcm3,
    A1z26Encode, A1z26Decode,
    RevbitsOf,
    LuminanceOf,
    ContrastRatio,
    SplitbillPerPerson,
    ElapsedPercent,

    // module "db" (SQLite databases)
    DbOpen,
    DbRun,
    DbValue,
    DbQuery,
    DbClose,
    // module "serve" (HTTP server)
    ServeStart,
    ServeWait,
    ServeReply,
    ServeStop,
    // module "proc" (background processes, OS-level concurrency)
    ProcStart,
    ProcDone,
    ProcWait,
    ProcKill,
    // module "gtq" (immediate-mode GUI widgets, inspired by Qt)
    GtqPanel,
    GtqLabel,
    GtqButton,
    GtqCheckbox,
    GtqSlider,
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

    // ---- batch A: unit conversions (all take one number and return one number) ----
    (AreaSqmToSqft, "area.sqm_to_sqft", Sig::Fixed(&[R], Some(Real))),
    (AreaSqftToSqm, "area.sqft_to_sqm", Sig::Fixed(&[R], Some(Real))),
    (AreaAcreToSqm, "area.acre_to_sqm", Sig::Fixed(&[R], Some(Real))),
    (AreaSqmToAcre, "area.sqm_to_acre", Sig::Fixed(&[R], Some(Real))),
    (AreaHectareToAcre, "area.hectare_to_acre", Sig::Fixed(&[R], Some(Real))),
    (AreaAcreToHectare, "area.acre_to_hectare", Sig::Fixed(&[R], Some(Real))),
    (VolLToGal, "volume.liter_to_gallon", Sig::Fixed(&[R], Some(Real))),
    (VolGalToL, "volume.gallon_to_liter", Sig::Fixed(&[R], Some(Real))),
    (VolLToMl, "volume.liter_to_ml", Sig::Fixed(&[R], Some(Real))),
    (VolMlToL, "volume.ml_to_liter", Sig::Fixed(&[R], Some(Real))),
    (VolCupToMl, "volume.cup_to_ml", Sig::Fixed(&[R], Some(Real))),
    (VolMlToCup, "volume.ml_to_cup", Sig::Fixed(&[R], Some(Real))),
    (SpeedKmhToMph, "speed.kmh_to_mph", Sig::Fixed(&[R], Some(Real))),
    (SpeedMphToKmh, "speed.mph_to_kmh", Sig::Fixed(&[R], Some(Real))),
    (SpeedMpsToKmh, "speed.mps_to_kmh", Sig::Fixed(&[R], Some(Real))),
    (SpeedKmhToMps, "speed.kmh_to_mps", Sig::Fixed(&[R], Some(Real))),
    (SpeedKnotToKmh, "speed.knot_to_kmh", Sig::Fixed(&[R], Some(Real))),
    (SpeedKmhToKnot, "speed.kmh_to_knot", Sig::Fixed(&[R], Some(Real))),
    (PaToBar, "pressure.pascal_to_bar", Sig::Fixed(&[R], Some(Real))),
    (BarToPa, "pressure.bar_to_pascal", Sig::Fixed(&[R], Some(Real))),
    (PsiToBar, "pressure.psi_to_bar", Sig::Fixed(&[R], Some(Real))),
    (BarToPsi, "pressure.bar_to_psi", Sig::Fixed(&[R], Some(Real))),
    (AtmToBar, "pressure.atm_to_bar", Sig::Fixed(&[R], Some(Real))),
    (BarToAtm, "pressure.bar_to_atm", Sig::Fixed(&[R], Some(Real))),
    (JouleToCal, "energy.joule_to_calorie", Sig::Fixed(&[R], Some(Real))),
    (CalToJoule, "energy.calorie_to_joule", Sig::Fixed(&[R], Some(Real))),
    (KwhToJoule, "energy.kwh_to_joule", Sig::Fixed(&[R], Some(Real))),
    (JouleToKwh, "energy.joule_to_kwh", Sig::Fixed(&[R], Some(Real))),
    (WattToHp, "power.watt_to_hp", Sig::Fixed(&[R], Some(Real))),
    (HpToWatt, "power.hp_to_watt", Sig::Fixed(&[R], Some(Real))),
    (MpgToL100, "fuel.mpg_to_l100km", Sig::Fixed(&[R], Some(Real))),
    (L100ToMpg, "fuel.l100km_to_mpg", Sig::Fixed(&[R], Some(Real))),
    (NewtonToLbf, "force.newton_to_lbf", Sig::Fixed(&[R], Some(Real))),
    (LbfToNewton, "force.lbf_to_newton", Sig::Fixed(&[R], Some(Real))),
    (NmToLbft, "torque.nm_to_lbft", Sig::Fixed(&[R], Some(Real))),
    (LbftToNm, "torque.lbft_to_nm", Sig::Fixed(&[R], Some(Real))),
    (MbpsToMbs, "datarate.mbps_to_mbs", Sig::Fixed(&[R], Some(Real))),
    (MbsToMbps, "datarate.mbs_to_mbps", Sig::Fixed(&[R], Some(Real))),
    (HzToRpm, "frequency.hz_to_rpm", Sig::Fixed(&[R], Some(Real))),
    (RpmToHz, "frequency.rpm_to_hz", Sig::Fixed(&[R], Some(Real))),
    (TbspToMl, "cooking.tbsp_to_ml", Sig::Fixed(&[R], Some(Real))),
    (MlToTbsp, "cooking.ml_to_tbsp", Sig::Fixed(&[R], Some(Real))),
    (TspToMl, "cooking.tsp_to_ml", Sig::Fixed(&[R], Some(Real))),
    (MlToTsp, "cooking.ml_to_tsp", Sig::Fixed(&[R], Some(Real))),
    (NmiToKm, "nautical.nmi_to_km", Sig::Fixed(&[R], Some(Real))),
    (KmToNmi, "nautical.km_to_nmi", Sig::Fixed(&[R], Some(Real))),
    (AuToKm, "astro.au_to_km", Sig::Fixed(&[R], Some(Real))),
    (KmToAu, "astro.km_to_au", Sig::Fixed(&[R], Some(Real))),
    (LyToKm, "astro.ly_to_km", Sig::Fixed(&[R], Some(Real))),
    (KmToLy, "astro.km_to_ly", Sig::Fixed(&[R], Some(Real))),
    (StoneToKg, "stone.stone_to_kg", Sig::Fixed(&[R], Some(Real))),
    (KgToStone, "stone.kg_to_stone", Sig::Fixed(&[R], Some(Real))),
    (Gcm3ToKgm3, "density.gcm3_to_kgm3", Sig::Fixed(&[R], Some(Real))),
    (Kgm3ToGcm3, "density.kgm3_to_gcm3", Sig::Fixed(&[R], Some(Real))),
    (GainToDb, "db.gain_to_db", Sig::Fixed(&[R], Some(Real))),
    (DbToGain, "db.db_to_gain", Sig::Fixed(&[R], Some(Real))),
    (BpmToMs, "tempo.bpm_to_ms", Sig::Fixed(&[R], Some(Real))),
    (MsToBpm, "tempo.ms_to_bpm", Sig::Fixed(&[R], Some(Real))),
    (DegToGrad, "grad.deg_to_grad", Sig::Fixed(&[R], Some(Real))),
    (GradToDeg, "grad.grad_to_deg", Sig::Fixed(&[R], Some(Real))),
    (RadToGrad, "grad.rad_to_grad", Sig::Fixed(&[R], Some(Real))),
    (GradToRad, "grad.grad_to_rad", Sig::Fixed(&[R], Some(Real))),
    (HoursToSeconds, "timeof.hours_to_seconds", Sig::Fixed(&[R], Some(Real))),
    (SecondsToHours, "timeof.seconds_to_hours", Sig::Fixed(&[R], Some(Real))),
    (DaysToSeconds, "timeof.days_to_seconds", Sig::Fixed(&[R], Some(Real))),
    (SecondsToDays, "timeof.seconds_to_days", Sig::Fixed(&[R], Some(Real))),
    (MinkmToKmh, "pace.minkm_to_kmh", Sig::Fixed(&[R], Some(Real))),
    (KmhToMinkm, "pace.kmh_to_minkm", Sig::Fixed(&[R], Some(Real))),

    // ---- batch B: maths ----
    (HyperSinh, "hyper.sinh", Sig::Fixed(&[R], Some(Real))),
    (HyperCosh, "hyper.cosh", Sig::Fixed(&[R], Some(Real))),
    (HyperTanh, "hyper.tanh", Sig::Fixed(&[R], Some(Real))),
    (AhyperAsinh, "ahyper.asinh", Sig::Fixed(&[R], Some(Real))),
    (AhyperAcosh, "ahyper.acosh", Sig::Fixed(&[R], Some(Real))),
    (AhyperAtanh, "ahyper.atanh", Sig::Fixed(&[R], Some(Real))),
    (LogsTwo, "logs.two", Sig::Fixed(&[R], Some(Real))),
    (LogsTen, "logs.ten", Sig::Fixed(&[R], Some(Real))),
    (LogsNatural, "logs.natural", Sig::Fixed(&[R], Some(Real))),
    (TrigxSec, "trigx.sec", Sig::Fixed(&[R], Some(Real))),
    (TrigxCsc, "trigx.csc", Sig::Fixed(&[R], Some(Real))),
    (TrigxCot, "trigx.cot", Sig::Fixed(&[R], Some(Real))),
    (ChooseCombinations, "choose.combinations", Sig::Fixed(&[I, I], Some(Int))),
    (ChoosePermutations, "choose.permutations", Sig::Fixed(&[I, I], Some(Int))),
    (SeqTriangular, "seq.triangular", Sig::Fixed(&[I], Some(Int))),
    (SeqSquare, "seq.square", Sig::Fixed(&[I], Some(Int))),
    (SeqCube, "seq.cube", Sig::Fixed(&[I], Some(Int))),
    (SeqCatalan, "seq.catalan", Sig::Fixed(&[I], Some(Int))),
    (SeqTetrahedral, "seq.tetrahedral", Sig::Fixed(&[I], Some(Int))),
    (ModularPow, "modular.pow", Sig::Fixed(&[I, I, I], Some(Int))),
    (LerpBetween, "lerp.between", Sig::Fixed(&[R, R, R], Some(Real))),
    (LerpMapRange, "lerp.map_range", Sig::Fixed(&[R, R, R, R, R], Some(Real))),
    (RoundingUp, "rounding.up", Sig::Fixed(&[R], Some(Int))),
    (RoundingDown, "rounding.down", Sig::Fixed(&[R], Some(Int))),
    (RoundingHalfUp, "rounding.half_up", Sig::Fixed(&[R], Some(Int))),
    (RoundingToMultiple, "rounding.to_multiple", Sig::Fixed(&[R, R], Some(Real))),
    (Vec2Length, "vec2.length", Sig::Fixed(&[R, R], Some(Real))),
    (Vec2Dot, "vec2.dot", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (Vec2Angle, "vec2.angle", Sig::Fixed(&[R, R], Some(Real))),
    (Vec3Length, "vec3.length", Sig::Fixed(&[R, R, R], Some(Real))),
    (Vec3Dot, "vec3.dot", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
    (ComplexAbs, "complex.abs", Sig::Fixed(&[R, R], Some(Real))),
    (ComplexArg, "complex.arg", Sig::Fixed(&[R, R], Some(Real))),
    (ParityIsEven, "parity.is_even", Sig::Fixed(&[I], Some(Bool))),
    (ParityIsOdd, "parity.is_odd", Sig::Fixed(&[I], Some(Bool))),
    (DivisibleBy, "divisible.by", Sig::Fixed(&[I, I], Some(Bool))),
    (DigitsSum, "digits.sum", Sig::Fixed(&[I], Some(Int))),
    (DigitsCount, "digits.count", Sig::Fixed(&[I], Some(Int))),
    (DigitsReverse, "digits.reverse", Sig::Fixed(&[I], Some(Int))),
    (CollatzSteps, "collatz.steps", Sig::Fixed(&[I], Some(Int))),
    (PerfectIs, "perfect.is", Sig::Fixed(&[I], Some(Bool))),
    (ArmstrongIs, "armstrong.is", Sig::Fixed(&[I], Some(Bool))),
    (PalindromeText, "palindrome.text", Sig::Fixed(&[S], Some(Bool))),
    (PalindromeNumber, "palindrome.number", Sig::Fixed(&[I], Some(Bool))),
    (RangeContains, "range.contains", Sig::Fixed(&[R, R, R], Some(Bool))),
    (PointDistance, "point.distance", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (PointManhattan, "point.manhattan", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (SlopeBetween, "slope.between", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (RootSquare, "root.square", Sig::Fixed(&[R], Some(Real))),
    (RootCube, "root.cube", Sig::Fixed(&[R], Some(Real))),
    (RootNth, "root.nth", Sig::Fixed(&[R, R], Some(Real))),

    // ---- batch C: more text ----
    (CountWords, "count.words", Sig::Fixed(&[S], Some(Int))),
    (CountLines, "count.lines", Sig::Fixed(&[S], Some(Int))),
    (CountChars, "count.chars", Sig::Fixed(&[S], Some(Int))),
    (CountVowels, "count.vowels", Sig::Fixed(&[S], Some(Int))),
    (TruncateTo, "truncate.to", Sig::Fixed(&[S, I], Some(Str))),
    (TruncateWords, "truncate.words", Sig::Fixed(&[S, I], Some(Str))),
    (IndentBy, "indent.by", Sig::Fixed(&[S, I], Some(Str))),
    (StripPrefix, "strip.prefix", Sig::Fixed(&[S, S], Some(Str))),
    (StripSuffix, "strip.suffix", Sig::Fixed(&[S, S], Some(Str))),
    (MaskAll, "mask.all", Sig::Fixed(&[S], Some(Str))),
    (MaskMiddle, "mask.middle", Sig::Fixed(&[S, I], Some(Str))),
    (InitialsOf, "initials.of", Sig::Fixed(&[S], Some(Str))),
    (AcronymOf, "acronym.of", Sig::Fixed(&[S], Some(Str))),
    (PigLatin, "pig.latin", Sig::Fixed(&[S], Some(Str))),
    (AtbashApply, "atbash.apply", Sig::Fixed(&[S], Some(Str))),
    (VigenereEncode, "vigenere.encode", Sig::Fixed(&[S, S], Some(Str))),
    (VigenereDecode, "vigenere.decode", Sig::Fixed(&[S, S], Some(Str))),
    (Rot47Apply, "rot47.apply", Sig::Fixed(&[S], Some(Str))),
    (BinEncode, "bin.encode", Sig::Fixed(&[S], Some(Str))),
    (BinDecode, "bin.decode", Sig::Fixed(&[S], Some(Str))),
    (Base32Encode, "base32.encode", Sig::Fixed(&[S], Some(Str))),
    (Base32Decode, "base32.decode", Sig::Fixed(&[S], Some(Str))),
    (HtmlEscape, "html.escape", Sig::Fixed(&[S], Some(Str))),
    (HtmlUnescape, "html.unescape", Sig::Fixed(&[S], Some(Str))),
    (ShoutIt, "shout.it", Sig::Fixed(&[S], Some(Str))),
    (ShoutWhisper, "shout.whisper", Sig::Fixed(&[S], Some(Str))),
    (WordsReverse, "words.reverse", Sig::Fixed(&[S], Some(Str))),
    (WordsFirst, "words.first", Sig::Fixed(&[S], Some(Str))),
    (WordsLast, "words.last", Sig::Fixed(&[S], Some(Str))),
    (UnicodeCodepoint, "unicode.codepoint", Sig::Fixed(&[S], Some(Int))),
    (UnicodeFromCodepoint, "unicode.from_codepoint", Sig::Fixed(&[I], Some(Str))),
    (UnicodeSize, "unicode.size", Sig::Fixed(&[S], Some(Int))),
    (NormalizeSpaces, "normalize.spaces", Sig::Fixed(&[S], Some(Str))),
    (AnagramOf, "anagram.of", Sig::Fixed(&[S, S], Some(Bool))),
    (FizzbuzzAt, "fizzbuzz.at", Sig::Fixed(&[I], Some(Str))),
    (QuoteAdd, "quote.add", Sig::Fixed(&[S], Some(Str))),
    (RleEncode, "rle.encode", Sig::Fixed(&[S], Some(Str))),
    (RleDecode, "rle.decode", Sig::Fixed(&[S], Some(Str))),
    (WrapAt, "wrap.at", Sig::Fixed(&[S, I], Some(Str))),
    (SpongebobMock, "spongebob.mock", Sig::Fixed(&[S], Some(Str))),
    (ClapText, "clap.text", Sig::Fixed(&[S], Some(Str))),

    // ---- batch D: science, finance, geometry, dates ----
    (BmiValue, "bmi.value", Sig::Fixed(&[R, R], Some(Real))),
    (BmiCategory, "bmi.category", Sig::Fixed(&[R], Some(Str))),
    (OhmVoltage, "ohm.voltage", Sig::Fixed(&[R, R], Some(Real))),
    (OhmCurrent, "ohm.current", Sig::Fixed(&[R, R], Some(Real))),
    (OhmResistance, "ohm.resistance", Sig::Fixed(&[R, R], Some(Real))),
    (OhmPower, "ohm.power", Sig::Fixed(&[R, R], Some(Real))),
    (PhysicsForce, "physics.force", Sig::Fixed(&[R, R], Some(Real))),
    (PhysicsKinetic, "physics.kinetic", Sig::Fixed(&[R, R], Some(Real))),
    (PhysicsMomentum, "physics.momentum", Sig::Fixed(&[R, R], Some(Real))),
    (WaveFrequency, "wave.frequency", Sig::Fixed(&[R, R], Some(Real))),
    (WaveWavelength, "wave.wavelength", Sig::Fixed(&[R, R], Some(Real))),
    (WavePeriod, "wave.period", Sig::Fixed(&[R], Some(Real))),
    (GravityOn, "gravity.on", Sig::Fixed(&[S], Some(Real))),
    (InterestSimple, "interest.simple", Sig::Fixed(&[R, R, R], Some(Real))),
    (InterestCompound, "interest.compound", Sig::Fixed(&[R, R, R], Some(Real))),
    (LoanPayment, "loan.payment", Sig::Fixed(&[R, R, I], Some(Real))),
    (TipAmount, "tip.amount", Sig::Fixed(&[R, R], Some(Real))),
    (TipTotal, "tip.total", Sig::Fixed(&[R, R], Some(Real))),
    (DiscountApply, "discount.apply", Sig::Fixed(&[R, R], Some(Real))),
    (DiscountSaved, "discount.saved", Sig::Fixed(&[R, R], Some(Real))),
    (RoiPercent, "roi.percent", Sig::Fixed(&[R, R], Some(Real))),
    (MoneyFormat, "money.format", Sig::Fixed(&[R], Some(Str))),
    (BearingBetween, "bearing.between", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (TriangleHypotenuse, "triangle.hypotenuse", Sig::Fixed(&[R, R], Some(Real))),
    (TriangleHeron, "triangle.heron", Sig::Fixed(&[R, R, R], Some(Real))),
    (TriangleIsRight, "triangle.is_right", Sig::Fixed(&[R, R, R], Some(Bool))),
    (CircleArea, "circle.area", Sig::Fixed(&[R], Some(Real))),
    (CircleCircumference, "circle.circumference", Sig::Fixed(&[R], Some(Real))),
    (CircleDiameter, "circle.diameter", Sig::Fixed(&[R], Some(Real))),
    (RectangleArea, "rectangle.area", Sig::Fixed(&[R, R], Some(Real))),
    (RectanglePerimeter, "rectangle.perimeter", Sig::Fixed(&[R, R], Some(Real))),
    (RectangleDiagonal, "rectangle.diagonal", Sig::Fixed(&[R, R], Some(Real))),
    (SphereVolume, "sphere.volume", Sig::Fixed(&[R], Some(Real))),
    (SphereSurface, "sphere.surface", Sig::Fixed(&[R], Some(Real))),
    (CylinderVolume, "cylinder.volume", Sig::Fixed(&[R, R], Some(Real))),
    (CylinderSurface, "cylinder.surface", Sig::Fixed(&[R, R], Some(Real))),
    (ConeVolume, "cone.volume", Sig::Fixed(&[R, R], Some(Real))),
    (ConeSurface, "cone.surface", Sig::Fixed(&[R, R], Some(Real))),
    (WeekdayOf, "weekday.of", Sig::Fixed(&[I], Some(Str))),
    (MonthName, "month.name", Sig::Fixed(&[I], Some(Str))),
    (MonthDays, "month.days", Sig::Fixed(&[I, I], Some(Int))),
    (LeapYear, "leap.year", Sig::Fixed(&[I], Some(Bool))),
    (AgeYears, "age.years", Sig::Fixed(&[I, I], Some(Int))),
    (ZodiacSign, "zodiac.sign", Sig::Fixed(&[I, I], Some(Str))),
    (SeasonOf, "season.of", Sig::Fixed(&[I], Some(Str))),
    (GradeLetter, "grade.letter", Sig::Fixed(&[R], Some(Str))),
    (LogicXor, "logic.xor", Sig::Fixed(&[Bool, Bool], Some(Bool))),
    (LogicImplies, "logic.implies", Sig::Fixed(&[Bool, Bool], Some(Bool))),
    (LogicNand, "logic.nand", Sig::Fixed(&[Bool, Bool], Some(Bool))),
    (LogicNor, "logic.nor", Sig::Fixed(&[Bool, Bool], Some(Bool))),
    (YesnoOf, "yesno.of", Sig::Fixed(&[Bool], Some(Str))),
    (ScientificNotation, "scientific.notation", Sig::Fixed(&[R], Some(Str))),
    (FractionToDecimal, "fraction.to_decimal", Sig::Fixed(&[I, I], Some(Real))),
    (FractionSimplify, "fraction.simplify", Sig::Fixed(&[I, I], Some(Str))),
    (HeartrateMax, "heartrate.max", Sig::Fixed(&[I], Some(Int))),
    (HeartrateZone, "heartrate.zone", Sig::Fixed(&[I, I], Some(Int))),

    // ---- batch E: hashing, validation, randomness, ids, utilities ----
    (FnvHash, "fnv.hash", Sig::Fixed(&[S], Some(Int))),
    (Djb2Hash, "djb2.hash", Sig::Fixed(&[S], Some(Int))),
    (SdbmHash, "sdbm.hash", Sig::Fixed(&[S], Some(Int))),
    (AdlerValue, "adler.value", Sig::Fixed(&[S], Some(Int))),
    (Ipv4Valid, "ipv4.valid", Sig::Fixed(&[S], Some(Bool))),
    (MacValid, "mac.valid", Sig::Fixed(&[S], Some(Bool))),
    (HexcolorValid, "hexcolor.valid", Sig::Fixed(&[S], Some(Bool))),
    (WeburlValid, "weburl.valid", Sig::Fixed(&[S], Some(Bool))),
    (DiceRoll, "dice.roll", Sig::Fixed(&[I], Some(Int))),
    (DiceMany, "dice.many", Sig::Fixed(&[I, I], Some(Int))),
    (CoinFlip, "coin.flip", Sig::Fixed(&[], Some(Bool))),
    (PickBetween, "pick.between", Sig::Fixed(&[I, I], Some(Int))),
    (PickChance, "pick.chance", Sig::Fixed(&[R], Some(Bool))),
    (TokenHex, "token.hex", Sig::Fixed(&[I], Some(Str))),
    (PasswordMake, "password.make", Sig::Fixed(&[I], Some(Str))),
    (NanoidMake, "nanoid.make", Sig::Fixed(&[], Some(Str))),
    (OtpCode, "otp.code", Sig::Fixed(&[], Some(Int))),
    (LoremWords, "lorem.words", Sig::Fixed(&[I], Some(Str))),
    (RandcolorHex, "randcolor.hex", Sig::Fixed(&[], Some(Str))),
    (FactorCount, "factor.count", Sig::Fixed(&[I], Some(Int))),
    (FactorSum, "factor.sum", Sig::Fixed(&[I], Some(Int))),
    (TotientOf, "totient.of", Sig::Fixed(&[I], Some(Int))),
    (CompareOf, "compare.of", Sig::Fixed(&[R, R], Some(Int))),
    (ClampintBetween, "clampint.between", Sig::Fixed(&[I, I, I], Some(Int))),
    (PowSquare, "pow.square", Sig::Fixed(&[R], Some(Real))),
    (PowCube, "pow.cube", Sig::Fixed(&[R], Some(Real))),
    (PowOf, "pow.of", Sig::Fixed(&[R, R], Some(Real))),
    (CapsFirst, "caps.first", Sig::Fixed(&[S], Some(Str))),
    (SwapcaseApply, "swapcase.apply", Sig::Fixed(&[S], Some(Str))),
    (ConsonantsCount, "consonants.count", Sig::Fixed(&[S], Some(Int))),
    (Point3Distance, "point3.distance", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
    (MidpointOf, "midpoint.of", Sig::Fixed(&[R, R], Some(Real))),
    (IsqrtOf, "isqrt.of", Sig::Fixed(&[I], Some(Int))),
    (CycleIndex, "cycle.index", Sig::Fixed(&[I, I], Some(Int))),
    (DigitrootOf, "digitroot.of", Sig::Fixed(&[I], Some(Int))),

    // ---- batch F: advanced geometry, finance, weather, physics ----
    (TrapezoidArea, "trapezoid.area", Sig::Fixed(&[R, R, R], Some(Real))),
    (EllipseArea, "ellipse.area", Sig::Fixed(&[R, R], Some(Real))),
    (EllipsePerimeter, "ellipse.perimeter", Sig::Fixed(&[R, R], Some(Real))),
    (PentagonArea, "pentagon.area", Sig::Fixed(&[R], Some(Real))),
    (HexagonArea, "hexagon.area", Sig::Fixed(&[R], Some(Real))),
    (PolygonInteriorAngle, "polygon.interior_angle", Sig::Fixed(&[I], Some(Real))),
    (PolygonArea, "polygon.area", Sig::Fixed(&[I, R], Some(Real))),
    (PyramidVolume, "pyramid.volume", Sig::Fixed(&[R, R], Some(Real))),
    (PrismVolume, "prism.volume", Sig::Fixed(&[R, R], Some(Real))),
    (TorusVolume, "torus.volume", Sig::Fixed(&[R, R], Some(Real))),
    (CuboidVolume, "cuboid.volume", Sig::Fixed(&[R, R, R], Some(Real))),
    (CuboidSurface, "cuboid.surface", Sig::Fixed(&[R, R, R], Some(Real))),
    (AnnulusArea, "annulus.area", Sig::Fixed(&[R, R], Some(Real))),
    (SavingsFutureValue, "savings.future_value", Sig::Fixed(&[R, R, I], Some(Real))),
    (DepreciationStraightLine, "depreciation.straight_line", Sig::Fixed(&[R, R, I], Some(Real))),
    (CentsToCents, "cents.to_cents", Sig::Fixed(&[R], Some(Int))),
    (CentsFromCents, "cents.from_cents", Sig::Fixed(&[I], Some(Real))),
    (MarkupPrice, "markup.price", Sig::Fixed(&[R, R], Some(Real))),
    (MarkupMargin, "markup.margin", Sig::Fixed(&[R, R], Some(Real))),
    (BmrMen, "bmr.men", Sig::Fixed(&[R, R, I], Some(Real))),
    (BmrWomen, "bmr.women", Sig::Fixed(&[R, R, I], Some(Real))),
    (WindChill, "wind.chill", Sig::Fixed(&[R, R], Some(Real))),
    (HeatIndex, "heat.index", Sig::Fixed(&[R, R], Some(Real))),
    (DewpointOf, "dewpoint.of", Sig::Fixed(&[R, R], Some(Real))),
    (QuadraticDiscriminant, "quadratic.discriminant", Sig::Fixed(&[R, R, R], Some(Real))),
    (QuadraticRoot1, "quadratic.root1", Sig::Fixed(&[R, R, R], Some(Real))),
    (QuadraticRoot2, "quadratic.root2", Sig::Fixed(&[R, R, R], Some(Real))),
    (MotionSpeed, "motion.speed", Sig::Fixed(&[R, R], Some(Real))),
    (MotionTime, "motion.time", Sig::Fixed(&[R, R], Some(Real))),
    (MotionDistance, "motion.distance", Sig::Fixed(&[R, R], Some(Real))),
    (AprToMonthly, "apr.to_monthly", Sig::Fixed(&[R], Some(Real))),
    (AprToAnnual, "apr.to_annual", Sig::Fixed(&[R], Some(Real))),

    // ---- module "grid": fast contiguous float arrays (grid type at the checker; int handle at run time) ----
    (GridRange, "nungw.range", Sig::Flexible { function: true }),
    (GridZeros, "nungw.zeros", Sig::Flexible { function: true }),
    (GridFill, "nungw.fill", Sig::Flexible { function: true }),
    (GridOf, "nungw.of", Sig::Flexible { function: true }),
    (GridSize, "nungw.size", Sig::Flexible { function: true }),
    (GridGet, "nungw.get", Sig::Flexible { function: true }),
    (GridSet, "nungw.set", Sig::Flexible { function: false }),
    (GridFree, "nungw.free", Sig::Flexible { function: false }),
    (GridToList, "nungw.to_list", Sig::Flexible { function: true }),
    (GridAdd, "nungw.add", Sig::Flexible { function: true }),
    (GridSub, "nungw.sub", Sig::Flexible { function: true }),
    (GridMul, "nungw.mul", Sig::Flexible { function: true }),
    (GridDiv, "nungw.div", Sig::Flexible { function: true }),
    (GridScale, "nungw.scale", Sig::Flexible { function: true }),
    (GridOffset, "nungw.offset", Sig::Flexible { function: true }),
    (GridSubScalar, "nungw.sub_scalar", Sig::Flexible { function: true }),
    (GridScalarSub, "nungw.scalar_sub", Sig::Flexible { function: true }),
    (GridDivScalar, "nungw.div_scalar", Sig::Flexible { function: true }),
    (GridScalarDiv, "nungw.scalar_div", Sig::Flexible { function: true }),
    (GridSqrt, "nungw.sqrt", Sig::Flexible { function: true }),
    (GridSquare, "nungw.square", Sig::Flexible { function: true }),
    (GridAbs, "nungw.abs", Sig::Flexible { function: true }),
    (GridSum, "nungw.sum", Sig::Flexible { function: true }),
    (GridMean, "nungw.mean", Sig::Flexible { function: true }),
    (GridMax, "nungw.max", Sig::Flexible { function: true }),
    (GridMin, "nungw.min", Sig::Flexible { function: true }),
    (GridDot, "nungw.dot", Sig::Flexible { function: true }),
    (GridSumSquares, "nungw.sum_squares", Sig::Flexible { function: true }),
    (GridNorm, "nungw.norm", Sig::Flexible { function: true }),
    (GridEval, "nungw.eval", Sig::Flexible { function: true }),
    (GridMatmul, "nungw.matmul", Sig::Flexible { function: true }),
    (CounterItems, "counter.items", Sig::Flexible { function: true }),
    (CounterChars, "counter.chars", Sig::Flexible { function: true }),
    (CounterWords, "counter.words", Sig::Flexible { function: true }),
    (WebPost, "web.post", Sig::Fixed(&[S, S], Some(Str))),
    (WebStatus, "web.status", Sig::Fixed(&[S], Some(Int))),
    (WebDownload, "web.download", Sig::Fixed(&[S, S], None)),
    (DataPretty, "data.pretty", Sig::Fixed(&[S], Some(Str))),
    (DataType, "data.type", Sig::Fixed(&[S, S], Some(Str))),
    (TextSplit, "text.split", Sig::Flexible { function: true }),
    (TextCount, "text.count", Sig::Fixed(&[S, S], Some(Int))),
    (TextGroup, "text.group", Sig::Fixed(&[S, S, I], Some(Str))),
    (ChartLine, "chart.line", Sig::Flexible { function: false }),
    (ChartBars, "chart.bars", Sig::Flexible { function: false }),
    (ChartScatter, "chart.scatter", Sig::Flexible { function: false }),
    (TableHeaders, "table.headers", Sig::Flexible { function: true }),
    (TableColumn, "table.column", Sig::Flexible { function: true }),
    (TableNumbers, "table.numbers", Sig::Flexible { function: true }),
    (TableRows, "table.rows", Sig::Fixed(&[S], Some(Int))),
    (TableMean, "table.mean", Sig::Fixed(&[S, S], Some(Real))),
    (TableSum, "table.sum", Sig::Fixed(&[S, S], Some(Real))),
    (IterCombinations, "iter.combinations", Sig::Flexible { function: true }),
    (IterPermutations, "iter.permutations", Sig::Flexible { function: true }),
    (IterProduct, "iter.product", Sig::Flexible { function: true }),
    (PickShuffle, "pick.shuffle", Sig::Flexible { function: false }),
    (PickChoice, "pick.choice", Sig::Flexible { function: true }),
    (PickSample, "pick.sample", Sig::Flexible { function: true }),
    (ClockAdd, "clock.add", Sig::Fixed(&[I, I], Some(Int))),
    (ClockDiff, "clock.diff", Sig::Fixed(&[I, I], Some(Int))),

    // ======== batch G1 ========
    (AccelGToMps2, "acceleration.g_to_mps2", Sig::Fixed(&[R], Some(Real))),
    (AccelMps2ToG, "acceleration.mps2_to_g", Sig::Fixed(&[R], Some(Real))),
    (LuxFcToLux, "lux.fc_to_lux", Sig::Fixed(&[R], Some(Real))),
    (LuxLuxToFc, "lux.lux_to_fc", Sig::Fixed(&[R], Some(Real))),
    (RadGrayToRad, "radiation.gray_to_rad", Sig::Fixed(&[R], Some(Real))),
    (RadRadToGray, "radiation.rad_to_gray", Sig::Fixed(&[R], Some(Real))),
    (RadSvToRem, "radiation.sievert_to_rem", Sig::Fixed(&[R], Some(Real))),
    (RadRemToSv, "radiation.rem_to_sievert", Sig::Fixed(&[R], Some(Real))),
    (TurnToDeg, "turn.to_deg", Sig::Fixed(&[R], Some(Real))),
    (TurnFromDeg, "turn.from_deg", Sig::Fixed(&[R], Some(Real))),
    (TurnToRad, "turn.to_rad", Sig::Fixed(&[R], Some(Real))),
    (TurnFromRad, "turn.from_rad", Sig::Fixed(&[R], Some(Real))),
    (FlowLpmToM3h, "flow.lpm_to_m3h", Sig::Fixed(&[R], Some(Real))),
    (FlowM3hToLpm, "flow.m3h_to_lpm", Sig::Fixed(&[R], Some(Real))),
    (TypoPtToMm, "typography.pt_to_mm", Sig::Fixed(&[R], Some(Real))),
    (TypoMmToPt, "typography.mm_to_pt", Sig::Fixed(&[R], Some(Real))),
    (TypoPtToInch, "typography.pt_to_inch", Sig::Fixed(&[R], Some(Real))),
    (TypoInchToPt, "typography.inch_to_pt", Sig::Fixed(&[R], Some(Real))),
    (OvenCToGas, "oven.c_to_gasmark", Sig::Fixed(&[R], Some(Real))),
    (OvenGasToC, "oven.gasmark_to_c", Sig::Fixed(&[R], Some(Real))),
    (GlucMgdlToMmol, "glucose.mgdl_to_mmol", Sig::Fixed(&[R], Some(Real))),
    (GlucMmolToMgdl, "glucose.mmol_to_mgdl", Sig::Fixed(&[R], Some(Real))),
    (ScreenPpi, "screen.ppi", Sig::Fixed(&[R, R, R], Some(Real))),
    (AspectRatio, "aspect.ratio", Sig::Fixed(&[R, R], Some(Real))),
    (BeaufortFromKmh, "beaufort.from_kmh", Sig::Fixed(&[R], Some(Int))),
    (QuakeEnergy, "quake.energy", Sig::Fixed(&[R], Some(Real))),
    (WrapvInto, "wrapv.into", Sig::Fixed(&[R, R, R], Some(Real))),
    (SmoothstepOf, "smoothstep.of", Sig::Fixed(&[R, R, R], Some(Real))),
    (StepAt, "step.at", Sig::Fixed(&[R, R], Some(Real))),
    (SincOf, "sinc.of", Sig::Fixed(&[R], Some(Real))),
    (SigmoidOf, "sigmoid.of", Sig::Fixed(&[R], Some(Real))),
    (ReluOf, "relu.of", Sig::Fixed(&[R], Some(Real))),
    (SoftsignOf, "softsign.of", Sig::Fixed(&[R], Some(Real))),
    (GaussPdf, "gauss.pdf", Sig::Fixed(&[R, R, R], Some(Real))),
    (ErfOf, "erf.of", Sig::Fixed(&[R], Some(Real))),
    (GammafOf, "gammaf.of", Sig::Fixed(&[R], Some(Real))),
    (HarmonicNumber, "harmonic.number", Sig::Fixed(&[I], Some(Real))),
    (GeoseriesSum, "geoseries.sum", Sig::Fixed(&[R, R, I], Some(Real))),
    (ArithseriesSum, "arithseries.sum", Sig::Fixed(&[R, R, I], Some(Real))),
    (ModuloWrap, "modulo.wrap", Sig::Fixed(&[I, I], Some(Int))),
    (ModuloFloordiv, "modulo.floordiv", Sig::Fixed(&[I, I], Some(Int))),
    (Hypot3Of, "hypot3.of", Sig::Fixed(&[R, R, R], Some(Real))),
    (ClampfBetween, "clampf.between", Sig::Fixed(&[R, R, R], Some(Real))),
    (FracPart, "frac.part", Sig::Fixed(&[R], Some(Real))),
    (FracWhole, "frac.whole", Sig::Fixed(&[R], Some(Real))),
    (ReciprocalOf, "reciprocal.of", Sig::Fixed(&[R], Some(Real))),
    (PreciseLog1p, "precise.log1p", Sig::Fixed(&[R], Some(Real))),
    (PreciseExpm1, "precise.expm1", Sig::Fixed(&[R], Some(Real))),
    (RotstrLeft, "rotstr.left", Sig::Fixed(&[S, I], Some(Str))),
    (RotstrRight, "rotstr.right", Sig::Fixed(&[S, I], Some(Str))),
    (ZfillOf, "zfill.of", Sig::Fixed(&[S, I], Some(Str))),
    (NospacesOf, "nospaces.of", Sig::Fixed(&[S], Some(Str))),
    (DigitsonlyOf, "digitsonly.of", Sig::Fixed(&[S], Some(Str))),
    (LettersonlyOf, "lettersonly.of", Sig::Fixed(&[S], Some(Str))),
    (DisemvowelOf, "disemvowel.of", Sig::Fixed(&[S], Some(Str))),
    (ScreamingOf, "screaming.of", Sig::Fixed(&[S], Some(Str))),
    (DotcaseOf, "dotcase.of", Sig::Fixed(&[S], Some(Str))),
    (PathcaseOf, "pathcase.of", Sig::Fixed(&[S], Some(Str))),
    (SentencecaseOf, "sentencecase.of", Sig::Fixed(&[S], Some(Str))),
    (DedupChars, "dedup.chars", Sig::Fixed(&[S], Some(Str))),
    (ExpandtabsOf, "expandtabs.of", Sig::Fixed(&[S, I], Some(Str))),
    (Rot5Digits, "rot5.digits", Sig::Fixed(&[S], Some(Str))),
    (HammingDistance, "hamming.distance", Sig::Fixed(&[S, S], Some(Int))),
    (LevenshteinDistance, "levenshtein.distance", Sig::Fixed(&[S, S], Some(Int))),
    (SoundexOf, "soundex.of", Sig::Fixed(&[S], Some(Str))),
    (SyllablesCount, "syllables.count", Sig::Fixed(&[S], Some(Int))),
    (AvgwordAverage, "avgword.average", Sig::Fixed(&[S], Some(Real))),
    (ReadtimeMinutes, "readtime.minutes", Sig::Fixed(&[S], Some(Real))),

    // ======== batch G2 ========
    (KinVfinal, "kinematics.final_velocity", Sig::Fixed(&[R, R, R], Some(Real))),
    (KinDistance, "kinematics.distance", Sig::Fixed(&[R, R, R], Some(Real))),
    (ProjRange, "projectile.range", Sig::Fixed(&[R, R], Some(Real))),
    (ProjMaxHeight, "projectile.max_height", Sig::Fixed(&[R, R], Some(Real))),
    (ProjFlightTime, "projectile.flight_time", Sig::Fixed(&[R, R], Some(Real))),
    (PendPeriod, "pendulum.period", Sig::Fixed(&[R], Some(Real))),
    (SpringPeriod, "spring.period", Sig::Fixed(&[R, R], Some(Real))),
    (SpringForce, "spring.force", Sig::Fixed(&[R, R], Some(Real))),
    (LensFocal, "lens.focal", Sig::Fixed(&[R, R], Some(Real))),
    (LensDiopter, "lens.diopter", Sig::Fixed(&[R], Some(Real))),
    (ResSeries, "resistors.series2", Sig::Fixed(&[R, R], Some(Real))),
    (ResParallel, "resistors.parallel2", Sig::Fixed(&[R, R], Some(Real))),
    (CapSeries, "capacitors.series2", Sig::Fixed(&[R, R], Some(Real))),
    (CapParallel, "capacitors.parallel2", Sig::Fixed(&[R, R], Some(Real))),
    (BuoyForce, "buoyancy.force", Sig::Fixed(&[R, R], Some(Real))),
    (SpecGravity, "specificgravity.of", Sig::Fixed(&[R], Some(Real))),
    (ThermoEnergy, "thermo.energy", Sig::Fixed(&[R, R, R], Some(Real))),
    (EfficiencyPercent, "efficiency.percent", Sig::Fixed(&[R, R], Some(Real))),
    (WorkJoules, "work.joules", Sig::Fixed(&[R, R], Some(Real))),
    (MomentOf, "moment.of", Sig::Fixed(&[R, R], Some(Real))),
    (ImpulseOf, "impulse.of", Sig::Fixed(&[R, R], Some(Real))),
    (DopplerObserved, "doppler.observed", Sig::Fixed(&[R, R, R], Some(Real))),
    (EscapeVelocity, "escape.velocity", Sig::Fixed(&[R, R], Some(Real))),
    (OrbitPeriod, "orbit.period", Sig::Fixed(&[R, R], Some(Real))),
    (RedshiftZ, "redshift.z", Sig::Fixed(&[R, R], Some(Real))),
    (LorentzFactor, "lorentz.factor", Sig::Fixed(&[R], Some(Real))),
    (HalflifeRemaining, "halflife.remaining", Sig::Fixed(&[R, R, R], Some(Real))),
    (DecayAmount, "decay.amount", Sig::Fixed(&[R, R, R], Some(Real))),
    (GrowthAmount, "growth.amount", Sig::Fixed(&[R, R, R], Some(Real))),
    (Rule72Years, "rule72.years", Sig::Fixed(&[R], Some(Real))),
    (AnnuityPv, "annuity.present_value", Sig::Fixed(&[R, R, I], Some(Real))),
    (BreakevenUnits, "breakeven.units", Sig::Fixed(&[R, R, R], Some(Real))),
    (ProfitAmount, "profit.amount", Sig::Fixed(&[R, R], Some(Real))),
    (ProfitMargin, "profit.margin", Sig::Fixed(&[R, R], Some(Real))),
    (CommissionOf, "commission.of", Sig::Fixed(&[R, R], Some(Real))),
    (WageWeekly, "wage.weekly", Sig::Fixed(&[R, R], Some(Real))),
    (WageOvertime, "wage.overtime", Sig::Fixed(&[R, R], Some(Real))),
    (InflationAdjust, "inflation.adjust", Sig::Fixed(&[R, R, R], Some(Real))),
    (IdealweightForHeight, "idealweight.for_height", Sig::Fixed(&[R], Some(Real))),
    (TdeeFromBmr, "tdee.from_bmr", Sig::Fixed(&[R, R], Some(Real))),
    (WaterDaily, "water.daily", Sig::Fixed(&[R], Some(Real))),
    (DosageByWeight, "dosage.by_weight", Sig::Fixed(&[R, R], Some(Real))),
    (GforceTurn, "gforce.turn", Sig::Fixed(&[R, R], Some(Real))),
    (GearRatio, "gear.ratio", Sig::Fixed(&[R, R], Some(Real))),
    (AutonomyDistance, "autonomy.distance", Sig::Fixed(&[R, R], Some(Real))),
    (EmissionsFromKm, "emissions.from_km", Sig::Fixed(&[R, R], Some(Real))),
    (SolarWatts, "solarpower.watts", Sig::Fixed(&[R, R, R], Some(Real))),
    (BatteryRuntime, "battery.runtime", Sig::Fixed(&[R, R], Some(Real))),
    (LedResistor, "led.resistor", Sig::Fixed(&[R, R, R], Some(Real))),
    (FreefallVelocity, "freefall.velocity", Sig::Fixed(&[R], Some(Real))),
    (FreefallDistance, "freefall.distance", Sig::Fixed(&[R], Some(Real))),
    (TetraVolume, "tetrahedron.volume", Sig::Fixed(&[R], Some(Real))),
    (TetraSurface, "tetrahedron.surface", Sig::Fixed(&[R], Some(Real))),
    (OctaVolume, "octahedron.volume", Sig::Fixed(&[R], Some(Real))),
    (ParallelogramArea, "parallelogram.area", Sig::Fixed(&[R, R], Some(Real))),
    (RhombusArea, "rhombus.area", Sig::Fixed(&[R, R], Some(Real))),
    (SectorArea, "sector.area", Sig::Fixed(&[R, R], Some(Real))),
    (SectorArc, "sector.arc", Sig::Fixed(&[R, R], Some(Real))),
    (EllipsoidVolume, "ellipsoid.volume", Sig::Fixed(&[R, R, R], Some(Real))),
    (NgonPerimeter, "ngon.perimeter", Sig::Fixed(&[I, R], Some(Real))),
    (NgonApothem, "ngon.apothem", Sig::Fixed(&[I, R], Some(Real))),
    (FrustumVolume, "frustum.volume", Sig::Fixed(&[R, R, R], Some(Real))),
    (InclinePctToDeg, "incline.percent_to_degrees", Sig::Fixed(&[R], Some(Real))),
    (InclineDegToPct, "incline.degrees_to_percent", Sig::Fixed(&[R], Some(Real))),

    // ======== batch G3 ========
    (ApproxEqual, "approx.equal", Sig::Fixed(&[R, R, R], Some(Bool))),
    (ProportionSolve, "proportion.solve", Sig::Fixed(&[R, R, R], Some(Real))),
    (MeansOf2, "means.of2", Sig::Fixed(&[R, R], Some(Real))),
    (MeansGeometric2, "means.geometric2", Sig::Fixed(&[R, R], Some(Real))),
    (MeansHarmonic2, "means.harmonic2", Sig::Fixed(&[R, R], Some(Real))),
    (BankersRound, "bankers.round", Sig::Fixed(&[R], Some(Int))),
    (PluralOf, "plural.of", Sig::Fixed(&[S, I], Some(Str))),
    (AbbreviateNumber, "abbreviate.number", Sig::Fixed(&[R], Some(Str))),
    (GpaFromPercent, "gpa.from_percent", Sig::Fixed(&[R], Some(Real))),
    (CurveGrade, "curve.grade", Sig::Fixed(&[R, R], Some(Real))),
    (ZscoreOf, "zscore.of", Sig::Fixed(&[R, R, R], Some(Real))),
    (MinmaxnormOf, "minmaxnorm.of", Sig::Fixed(&[R, R, R], Some(Real))),
    (Clamp01Of, "clamp01.of", Sig::Fixed(&[R], Some(Real))),
    (CerpOf, "cerp.of", Sig::Fixed(&[R, R, R], Some(Real))),
    (NibbleHigh, "nibble.high", Sig::Fixed(&[I], Some(Int))),
    (NibbleLow, "nibble.low", Sig::Fixed(&[I], Some(Int))),
    (ByteopsHigh, "byteops.high", Sig::Fixed(&[I], Some(Int))),
    (ByteopsLow, "byteops.low", Sig::Fixed(&[I], Some(Int))),
    (EndianSwap16, "endian.swap16", Sig::Fixed(&[I], Some(Int))),
    (EndianSwap32, "endian.swap32", Sig::Fixed(&[I], Some(Int))),
    (Nextpow2Of, "nextpow2.of", Sig::Fixed(&[I], Some(Int))),
    (Ispow2Of, "ispow2.of", Sig::Fixed(&[I], Some(Bool))),
    (AlignUp, "align.up", Sig::Fixed(&[I, I], Some(Int))),
    (AlignDown, "align.down", Sig::Fixed(&[I, I], Some(Int))),
    (GraycodeEncode, "graycode.encode", Sig::Fixed(&[I], Some(Int))),
    (GraycodeDecode, "graycode.decode", Sig::Fixed(&[I], Some(Int))),
    (EanCheckDigit, "ean.check_digit", Sig::Fixed(&[S], Some(Int))),
    (CardtypeOf, "cardtype.of", Sig::Fixed(&[S], Some(Str))),
    (HumidexOf, "humidex.of", Sig::Fixed(&[R, R], Some(Real))),
    (PhFromConcentration, "ph.from_concentration", Sig::Fixed(&[R], Some(Real))),
    (PhConcentration, "ph.concentration", Sig::Fixed(&[R], Some(Real))),
    (SoundAdd, "sound.add", Sig::Fixed(&[R, R], Some(Real))),
    (NoteFrequency, "note.frequency", Sig::Fixed(&[R], Some(Real))),
    (NoteCents, "note.cents", Sig::Fixed(&[R, R], Some(Real))),
    (ProbAnd, "prob.and", Sig::Fixed(&[R, R], Some(Real))),
    (ProbOr, "prob.or", Sig::Fixed(&[R, R], Some(Real))),
    (ProbInvert, "prob.invert", Sig::Fixed(&[R], Some(Real))),
    (OddsToProb, "odds.to_prob", Sig::Fixed(&[R], Some(Real))),
    (OddsFromProb, "odds.from_prob", Sig::Fixed(&[R], Some(Real))),
    (CollatzmaxPeak, "collatzmax.peak", Sig::Fixed(&[I], Some(Int))),
    (HappyIs, "happy.is", Sig::Fixed(&[I], Some(Bool))),
    (AliquotIsAbundant, "aliquot.is_abundant", Sig::Fixed(&[I], Some(Bool))),
    (AliquotIsDeficient, "aliquot.is_deficient", Sig::Fixed(&[I], Some(Bool))),
    (AliquotIsPerfect, "aliquot.is_perfect", Sig::Fixed(&[I], Some(Bool))),
    (PolygonalNumber, "polygonal.number", Sig::Fixed(&[I, I], Some(Int))),
    (PronicNumber, "pronic.number", Sig::Fixed(&[I], Some(Int))),
    (PentagonalNumber, "pentagonal.number", Sig::Fixed(&[I], Some(Int))),
    (HexagonalNumber, "hexagonal.number", Sig::Fixed(&[I], Some(Int))),
    (StarNumber, "star.number", Sig::Fixed(&[I], Some(Int))),
    (LucasNumber, "lucas.number", Sig::Fixed(&[I], Some(Int))),
    (TribonacciNumber, "tribonacci.number", Sig::Fixed(&[I], Some(Int))),
    (PadovanNumber, "padovan.number", Sig::Fixed(&[I], Some(Int))),
    (PrimorialOf, "primorial.of", Sig::Fixed(&[I], Some(Int))),
    (PrimecountBelow, "primecount.below", Sig::Fixed(&[I], Some(Int))),
    (PrevprimeBefore, "prevprime.before", Sig::Fixed(&[I], Some(Int))),
    (CoprimeAre, "coprime.are", Sig::Fixed(&[I, I], Some(Bool))),
    (AmicableAre, "amicable.are", Sig::Fixed(&[I, I], Some(Bool))),
    (MultiGcd3, "multi.gcd3", Sig::Fixed(&[I, I, I], Some(Int))),
    (MultiLcm3, "multi.lcm3", Sig::Fixed(&[I, I, I], Some(Int))),
    (A1z26Encode, "a1z26.encode", Sig::Fixed(&[S], Some(Str))),
    (A1z26Decode, "a1z26.decode", Sig::Fixed(&[S], Some(Str))),
    (RevbitsOf, "revbits.of", Sig::Fixed(&[I, I], Some(Int))),
    (LuminanceOf, "luminance.of", Sig::Fixed(&[S], Some(Real))),
    (ContrastRatio, "contrast.ratio", Sig::Fixed(&[S, S], Some(Real))),
    (SplitbillPerPerson, "splitbill.per_person", Sig::Fixed(&[R, I], Some(Real))),
    (ElapsedPercent, "elapsed.percent", Sig::Fixed(&[R, R], Some(Real))),

    // module "db" (SQLite)
    (DbOpen, "db.open", Sig::Fixed(&[S], Some(Int))),
    (DbRun, "db.run", Sig::Fixed(&[I, S], Some(Int))),
    (DbValue, "db.value", Sig::Fixed(&[I, S], Some(Str))),
    (DbQuery, "db.query", Sig::Fixed(&[I, S], Some(Str))),
    (DbClose, "db.close", Sig::Fixed(&[I], None)),

    // module "serve" (HTTP server)
    (ServeStart, "serve.start", Sig::Fixed(&[I], Some(Int))),
    (ServeWait, "serve.wait", Sig::Fixed(&[I], Some(Str))),
    (ServeReply, "serve.reply", Sig::Fixed(&[I, I, S], None)),
    (ServeStop, "serve.stop", Sig::Fixed(&[I], None)),

    // module "proc" (background processes)
    (ProcStart, "proc.start", Sig::Fixed(&[S], Some(Int))),
    (ProcDone, "proc.done", Sig::Fixed(&[I], Some(Bool))),
    (ProcWait, "proc.wait", Sig::Fixed(&[I], Some(Str))),
    (ProcKill, "proc.kill", Sig::Fixed(&[I], None)),

    // module "gtq" (immediate-mode GUI)
    (GtqPanel, "gtq.panel", Sig::Fixed(&[R, R, R, R, S], None)),
    (GtqLabel, "gtq.label", Sig::Fixed(&[R, R, S, S], None)),
    (GtqButton, "gtq.button", Sig::Fixed(&[R, R, R, R, S], Some(Bool))),
    (GtqCheckbox, "gtq.checkbox", Sig::Fixed(&[R, R, S, Bool], Some(Bool))),
    (GtqSlider, "gtq.slider", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
];

/// Built-in modules provided by the interpreter (imported with `import <name>`).
pub const BUILTIN_MODULES: &[&str] = &[
    "text", "hash", "web", "data", "clock", "encode", "shell", "math", "bits", "base", "roman", "temp", "angle",
    "percent", "geom", "length", "weight", "size", "duration", "geo", "ordinal", "fib", "prime", "str", "hex", "url",
    "case", "slug", "ascii", "morse", "caesar", "rot13", "leet", "nato", "color", "valid", "luhn", "isbn", "uuid",
    "crc", "env", "path", "os", "dir", "file", "tmp", "args", "csv", "stats", "tax", "compass",
    // batch A
    "area", "volume", "speed", "pressure", "energy", "power", "fuel", "force", "torque", "datarate", "frequency",
    "cooking", "nautical", "astro", "stone", "density", "db", "tempo", "grad", "timeof", "pace",
    // batch B
    "hyper", "ahyper", "logs", "trigx", "choose", "seq", "modular", "lerp", "rounding", "vec2", "vec3", "complex",
    "parity", "divisible", "digits", "collatz", "perfect", "armstrong", "palindrome", "range", "point", "slope", "root",
    // batch C
    "count", "truncate", "indent", "strip", "mask", "initials", "acronym", "pig", "atbash", "vigenere", "rot47", "bin",
    "base32", "html", "shout", "words", "unicode", "normalize", "anagram", "fizzbuzz", "quote", "rle", "wrap",
    "spongebob", "clap",
    // batch D
    "bmi", "ohm", "physics", "wave", "gravity", "interest", "loan", "tip", "discount", "roi", "money", "bearing",
    "triangle", "circle", "rectangle", "sphere", "cylinder", "cone", "weekday", "month", "leap", "age", "zodiac",
    "season", "grade", "logic", "yesno", "scientific", "fraction", "heartrate",
    // batch E
    "fnv", "djb2", "sdbm", "adler", "ipv4", "mac", "hexcolor", "weburl", "dice", "coin", "pick", "token", "password",
    "nanoid", "otp", "lorem", "randcolor", "factor", "totient", "compare", "clampint", "pow", "caps", "swapcase",
    "consonants", "point3", "midpoint", "isqrt", "cycle", "digitroot",
    // batch F
    "trapezoid", "ellipse", "pentagon", "hexagon", "polygon", "pyramid", "prism", "torus", "cuboid", "annulus",
    "savings", "depreciation", "cents", "markup", "bmr", "wind", "heat", "dewpoint", "quadratic", "motion", "apr",
    // array engine
    "nungw",
    // collections
    "counter",
    // plotting
    "chart",
    // tables and iterators
    "table", "iter",
    // batch G1
    "acceleration", "lux", "radiation", "turn", "flow", "typography", "oven", "glucose", "screen", "aspect",
    "beaufort", "quake", "wrapv", "smoothstep", "step", "sinc", "sigmoid", "relu", "softsign", "gauss", "erf",
    "gammaf", "harmonic", "geoseries", "arithseries", "modulo", "hypot3", "clampf", "frac", "reciprocal", "precise",
    "rotstr", "zfill", "nospaces", "digitsonly", "lettersonly", "disemvowel", "screaming", "dotcase", "pathcase",
    "sentencecase", "dedup", "expandtabs", "rot5", "hamming", "levenshtein", "soundex", "syllables", "avgword",
    "readtime",
    // batch G2
    "kinematics", "projectile", "pendulum", "spring", "lens", "resistors", "capacitors", "buoyancy", "specificgravity",
    "thermo", "efficiency", "work", "moment", "impulse", "doppler", "escape", "orbit", "redshift", "lorentz",
    "halflife", "decay", "growth", "rule72", "annuity", "breakeven", "profit", "commission", "wage", "inflation",
    "idealweight", "tdee", "water", "dosage", "gforce", "gear", "autonomy", "emissions", "solarpower", "battery",
    "led", "freefall", "tetrahedron", "octahedron", "parallelogram", "rhombus", "sector", "ellipsoid", "ngon",
    "frustum", "incline",
    // batch G3
    "approx", "proportion", "means", "bankers", "plural", "abbreviate", "gpa", "curve", "zscore", "minmaxnorm",
    "clamp01", "cerp", "nibble", "byteops", "endian", "nextpow2", "ispow2", "align", "graycode", "ean", "cardtype",
    "humidex", "ph", "sound", "note", "prob", "odds", "collatzmax", "happy", "aliquot", "polygonal", "pronic",
    "pentagonal", "hexagonal", "star", "lucas", "tribonacci", "padovan", "primorial", "primecount", "prevprime",
    "coprime", "amicable", "multi", "a1z26", "revbits", "luminance", "contrast", "splitbill", "elapsed",
    // databases, servers, processes, GUI
    "db", "serve", "proc", "gtq",
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
    /// the mouse button state at the previous frame, for `gtq` click detection
    prev_down: bool,
}

impl Default for Env {
    fn default() -> Env {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64);
        Env { graphics: None, fonts: Fonts::default(), start: Instant::now(), rng: seed | 1, prev_down: false }
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
                self.prev_down = self.graphics.as_ref().is_some_and(Graphics::click);
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
            GtqPanel => {
                let c = parse_color(text(&args[4]))?;
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                self.window()?.canvas.rect(x, y, w, h, c);
                Value::Null
            }
            GtqLabel => {
                let c = parse_color(text(&args[3]))?;
                let (x, y) = (real(&args[0]), real(&args[1]));
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                self.fonts.draw(&mut g.canvas, x, y, text(&args[2]), c)?;
                Value::Null
            }
            GtqButton => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let label = text(&args[4]).to_string();
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let pressed = hover && down && !self.prev_down;
                let bg = parse_color(if hover {
                    if down {
                        "#2d3748"
                    } else {
                        "#4a5568"
                    }
                } else {
                    "#374151"
                })?;
                let white = parse_color("#ffffff")?;
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                let tx = (x + (w - tw) / 2.0).max(x + 2.0);
                self.fonts.draw(&mut g.canvas, tx, y + h / 2.0 - 6.0, &label, white)?;
                Value::Bool(pressed)
            }
            GtqCheckbox => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let checked = matches!(&args[3], Value::Bool(true));
                let box_sz = 16.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + box_sz && my >= y && my <= y + box_sz, g.click())
                };
                let new_checked = if hover && down && !self.prev_down { !checked } else { checked };
                let border = parse_color("#888888")?;
                let fill = parse_color("#374151")?;
                let mark = parse_color("#22c55e")?;
                let white = parse_color("#dddddd")?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, box_sz, box_sz, fill);
                g.canvas.line(x, y, x + box_sz, y, border);
                g.canvas.line(x, y + box_sz, x + box_sz, y + box_sz, border);
                g.canvas.line(x, y, x, y + box_sz, border);
                g.canvas.line(x + box_sz, y, x + box_sz, y + box_sz, border);
                if new_checked {
                    g.canvas.rect(x + 4.0, y + 4.0, box_sz - 8.0, box_sz - 8.0, mark);
                }
                self.fonts.draw(&mut g.canvas, x + box_sz + 8.0, y + 2.0, &label, white)?;
                Value::Bool(new_checked)
            }
            GtqSlider => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let (val, lo, hi) = (real(&args[3]), real(&args[4]), real(&args[5]));
                let (hover, down, mxf) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (my >= y - 8.0 && my <= y + 8.0 && mx >= x && mx <= x + w, g.click(), mx)
                };
                let mut v = val;
                if hover && down && hi > lo && w > 0.0 {
                    v = lo + ((mxf - x) / w).clamp(0.0, 1.0) * (hi - lo);
                }
                let track = parse_color("#374151")?;
                let knob = parse_color("#60a5fa")?;
                let t = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y - 2.0, w, 4.0, track);
                g.canvas.circle(x + t * w, y, 8.0, knob);
                Value::Real(v)
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
