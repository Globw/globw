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

    // ======== batch H1: easing, advanced trig, more conversions ========
    EaseInQuad, EaseOutQuad, EaseInOutQuad, EaseInCubic, EaseOutCubic, EaseInOutCubic,
    EaseInQuart, EaseOutQuart, EaseInOutQuart, EaseInQuint, EaseOutQuint, EaseInOutQuint,
    EaseInSine, EaseOutSine, EaseInOutSine, EaseInExpo, EaseOutExpo, EaseInOutExpo,
    EaseInCirc, EaseOutCirc, EaseInOutCirc, EaseInBack, EaseOutBack, EaseInOutBack,
    EaseInElastic, EaseOutElastic, EaseInOutElastic, EaseInBounce, EaseOutBounce, EaseInOutBounce,
    TrigHaversin, TrigVersin, TrigCoversin, TrigExsec, TrigExcosec, TrigCrd,
    SpecialStirling, SpecialLogit, SpecialSoftplus, SpecialGudermann,
    PolarX, PolarY, PolarR, PolarAngle,
    TransformRotX, TransformRotY,
    Temp2CToRankine, Temp2RankineToC, Temp2FToRankine, Temp2RankineToF, Temp2CToReaumur, Temp2ReaumurToC,
    StorageBytesToKb, StorageKbToMb, StorageMbToGb, StorageGbToTb, StorageTbToPb, StorageBitsToBytes,
    Area2AcresToHa, Area2HaToAcres, Area2SqkmToSqmi, Area2SqmiToSqkm,
    KitchenTspToTbsp, KitchenTbspToCup, KitchenCupToFloz, KitchenFlozToMl, KitchenMlToFloz, KitchenOzToGrams, KitchenGramsToOz, KitchenLbToOz,

    // ======== batch H2: chemistry, electronics, finance, health, science ========
    ChemMolarity, ChemMoles, ChemMass, ChemDilution, ChemPoh, ChemPercentYield, ChemGasPressure, ChemDensity, ChemAtoms, ChemMolesFromAtoms,
    ElecPower, ElecEnergy, ElecChargeCap, ElecInductorEnergy, ElecRcTime, ElecReactanceC, ElecReactanceL, ElecResonance,
    Fin2Cagr, Fin2Pv, Fin2Fv, Fin2EffectiveRate, Fin2RealReturn, Fin2DebtRatio, Fin2CurrentRatio, Fin2Roe, Fin2Eps, Fin2PeRatio,
    H2Bsa, H2WaistHip, H2TargetHr, H2CaloriesWalk, H2CaloriesRun, H2Vo2max,
    MusicSemitonesBetween, MusicTranspose, MusicBpmToHz, MusicHzToBpm, MusicCentsToRatio, MusicRatioToCents,
    SpaceGravForce, SpaceSchwarzschild, SpaceOrbitalVel, SpaceSurfaceGravity,
    OpticsMagnification, OpticsFocalRadius, OpticsLensPower, OpticsSnell,
    Prob2Binomial, Prob2Poisson, Prob2NormalCdf, Prob2NormalPdf,
    RpgDamage, RpgXpForLevel, RpgLevelFromXp, RpgCrit, RpgHitChance,
    Phys2EnergyMass, Phys2PhotonEnergy, Phys2DeBroglie, Phys2Coulomb,
    Stats2Cv, Stats2Se, Stats2Margin,
    MapMercatorY, MapTileX, MapTileY,
    Date2DayOfYear, Date2Zeller, Date2DaysInYear,
    Wave2Angular, Wave2Period, Wave2Frequency,
    LightSpeed, LightTimeToTravel, LightDistanceLy, LightYearsToKm,
    Sound2Speed, Sound2DistFromDelay, Sound2TimeForDist,

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
    GtqRadio,
    GtqToggle,
    GtqProgress,
    GtqSeparator,
    GtqGroupbox,
    GtqSpinbox,
    GtqSelector,
    GtqTextbox,
    GtqVslider,
    GtqHovered,
    GtqClicked,
    // gtq batch G: pure colour helpers
    GtqLighten, GtqDarken, GtqMix, GtqGrayscale, GtqInvert, GtqComplement, GtqTint, GtqShade,
    GtqContrastColor, GtqFromRgb, GtqFromHsl, GtqHueShift, GtqSaturate, GtqDesaturate,
    GtqWithRed, GtqWithGreen, GtqWithBlue, GtqBrighten, GtqDim, GtqMute,
    GtqRedOf, GtqGreenOf, GtqBlueOf, GtqLuminanceOf, GtqIsDark,
    // gtq batch G: pure layout helpers
    GtqRowX, GtqColY, GtqGridX, GtqGridY, GtqCenterX, GtqCenterY, GtqRightX, GtqBottomY,
    GtqFitW, GtqFitH, GtqAspectW, GtqAspectH, GtqLerpPos, GtqRemap, GtqSnap, GtqGolden,
    GtqPadIn, GtqInsetW, GtqDistribute, GtqSpanEnd, GtqScaleAt, GtqWrapIndex, GtqClampPos, GtqCellX, GtqCellY,
    // gtq batch G: vector icons
    GtqIcon,
    GtqIcCheck, GtqIcClose, GtqIcPlus, GtqIcMinus, GtqIcUp, GtqIcDown, GtqIcLeft, GtqIcRight,
    GtqIcChevronUp, GtqIcChevronDown, GtqIcChevronLeft, GtqIcChevronRight,
    GtqIcMenu, GtqIcKebab, GtqIcGear, GtqIcHome, GtqIcSearch, GtqIcHeart, GtqIcStar, GtqIcUser,
    GtqIcBell, GtqIcMail, GtqIcTrash, GtqIcPencil, GtqIcSave, GtqIcFolder, GtqIcFile,
    GtqIcPlay, GtqIcPause, GtqIcStop, GtqIcNext, GtqIcPrev, GtqIcVolume, GtqIcMute,
    GtqIcWifi, GtqIcBattery, GtqIcLock, GtqIcUnlock, GtqIcEye, GtqIcEyeOff,
    GtqIcCalendar, GtqIcClock, GtqIcPin, GtqIcTag, GtqIcFlag, GtqIcBookmark,
    GtqIcDownload, GtqIcUpload, GtqIcRefresh,
    // gtq batch G: more widgets (display)
    GtqCard, GtqCardHeader, GtqBadge, GtqChip, GtqPill, GtqAvatar, GtqStatusDot, GtqVdivider,
    GtqTooltip, GtqToast, GtqGauge, GtqMeter, GtqSpinner, GtqTitlebar, GtqStatusbar, GtqToolbar,
    GtqBreadcrumb, GtqKbd, GtqProgressRing, GtqSkeleton, GtqImagePlaceholder, GtqRibbon, GtqNote, GtqBar,
    // gtq batch G: more widgets (interactive)
    GtqIconButton, GtqLink, GtqSwitch, GtqStepper, GtqSegmented, GtqTabs, GtqNumberbox, GtqListItem,
    GtqMenuItem, GtqAccordion, GtqRating, GtqColorSwatch, GtqChipClose, GtqFab, GtqPagination,
    GtqButtonPrimary, GtqButtonGhost, GtqKnob, GtqScrollbarV, GtqDropdown, GtqCheckboxCard,
    GtqToggleIcon, GtqTextButton, GtqSliderMini, GtqStepperF, GtqCloseButton,
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
    // ======== batch H1 ========
    (EaseInQuad, "ease.in_quad", Sig::Fixed(&[R], Some(Real))),
    (EaseOutQuad, "ease.out_quad", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutQuad, "ease.in_out_quad", Sig::Fixed(&[R], Some(Real))),
    (EaseInCubic, "ease.in_cubic", Sig::Fixed(&[R], Some(Real))),
    (EaseOutCubic, "ease.out_cubic", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutCubic, "ease.in_out_cubic", Sig::Fixed(&[R], Some(Real))),
    (EaseInQuart, "ease.in_quart", Sig::Fixed(&[R], Some(Real))),
    (EaseOutQuart, "ease.out_quart", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutQuart, "ease.in_out_quart", Sig::Fixed(&[R], Some(Real))),
    (EaseInQuint, "ease.in_quint", Sig::Fixed(&[R], Some(Real))),
    (EaseOutQuint, "ease.out_quint", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutQuint, "ease.in_out_quint", Sig::Fixed(&[R], Some(Real))),
    (EaseInSine, "ease.in_sine", Sig::Fixed(&[R], Some(Real))),
    (EaseOutSine, "ease.out_sine", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutSine, "ease.in_out_sine", Sig::Fixed(&[R], Some(Real))),
    (EaseInExpo, "ease.in_expo", Sig::Fixed(&[R], Some(Real))),
    (EaseOutExpo, "ease.out_expo", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutExpo, "ease.in_out_expo", Sig::Fixed(&[R], Some(Real))),
    (EaseInCirc, "ease.in_circ", Sig::Fixed(&[R], Some(Real))),
    (EaseOutCirc, "ease.out_circ", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutCirc, "ease.in_out_circ", Sig::Fixed(&[R], Some(Real))),
    (EaseInBack, "ease.in_back", Sig::Fixed(&[R], Some(Real))),
    (EaseOutBack, "ease.out_back", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutBack, "ease.in_out_back", Sig::Fixed(&[R], Some(Real))),
    (EaseInElastic, "ease.in_elastic", Sig::Fixed(&[R], Some(Real))),
    (EaseOutElastic, "ease.out_elastic", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutElastic, "ease.in_out_elastic", Sig::Fixed(&[R], Some(Real))),
    (EaseInBounce, "ease.in_bounce", Sig::Fixed(&[R], Some(Real))),
    (EaseOutBounce, "ease.out_bounce", Sig::Fixed(&[R], Some(Real))),
    (EaseInOutBounce, "ease.in_out_bounce", Sig::Fixed(&[R], Some(Real))),
    (TrigHaversin, "trig2.haversin", Sig::Fixed(&[R], Some(Real))),
    (TrigVersin, "trig2.versin", Sig::Fixed(&[R], Some(Real))),
    (TrigCoversin, "trig2.coversin", Sig::Fixed(&[R], Some(Real))),
    (TrigExsec, "trig2.exsec", Sig::Fixed(&[R], Some(Real))),
    (TrigExcosec, "trig2.excosec", Sig::Fixed(&[R], Some(Real))),
    (TrigCrd, "trig2.chord", Sig::Fixed(&[R], Some(Real))),
    (SpecialStirling, "special.stirling", Sig::Fixed(&[R], Some(Real))),
    (SpecialLogit, "special.logit", Sig::Fixed(&[R], Some(Real))),
    (SpecialSoftplus, "special.softplus", Sig::Fixed(&[R], Some(Real))),
    (SpecialGudermann, "special.gudermannian", Sig::Fixed(&[R], Some(Real))),
    (PolarX, "polar.to_x", Sig::Fixed(&[R, R], Some(Real))),
    (PolarY, "polar.to_y", Sig::Fixed(&[R, R], Some(Real))),
    (PolarR, "polar.to_r", Sig::Fixed(&[R, R], Some(Real))),
    (PolarAngle, "polar.to_angle", Sig::Fixed(&[R, R], Some(Real))),
    (TransformRotX, "transform.rotate_x", Sig::Fixed(&[R, R, R], Some(Real))),
    (TransformRotY, "transform.rotate_y", Sig::Fixed(&[R, R, R], Some(Real))),
    (Temp2CToRankine, "temp2.c_to_rankine", Sig::Fixed(&[R], Some(Real))),
    (Temp2RankineToC, "temp2.rankine_to_c", Sig::Fixed(&[R], Some(Real))),
    (Temp2FToRankine, "temp2.f_to_rankine", Sig::Fixed(&[R], Some(Real))),
    (Temp2RankineToF, "temp2.rankine_to_f", Sig::Fixed(&[R], Some(Real))),
    (Temp2CToReaumur, "temp2.c_to_reaumur", Sig::Fixed(&[R], Some(Real))),
    (Temp2ReaumurToC, "temp2.reaumur_to_c", Sig::Fixed(&[R], Some(Real))),
    (StorageBytesToKb, "storage.bytes_to_kb", Sig::Fixed(&[R], Some(Real))),
    (StorageKbToMb, "storage.kb_to_mb", Sig::Fixed(&[R], Some(Real))),
    (StorageMbToGb, "storage.mb_to_gb", Sig::Fixed(&[R], Some(Real))),
    (StorageGbToTb, "storage.gb_to_tb", Sig::Fixed(&[R], Some(Real))),
    (StorageTbToPb, "storage.tb_to_pb", Sig::Fixed(&[R], Some(Real))),
    (StorageBitsToBytes, "storage.bits_to_bytes", Sig::Fixed(&[R], Some(Real))),
    (Area2AcresToHa, "area2.acres_to_hectares", Sig::Fixed(&[R], Some(Real))),
    (Area2HaToAcres, "area2.hectares_to_acres", Sig::Fixed(&[R], Some(Real))),
    (Area2SqkmToSqmi, "area2.sqkm_to_sqmi", Sig::Fixed(&[R], Some(Real))),
    (Area2SqmiToSqkm, "area2.sqmi_to_sqkm", Sig::Fixed(&[R], Some(Real))),
    (KitchenTspToTbsp, "kitchen.tsp_to_tbsp", Sig::Fixed(&[R], Some(Real))),
    (KitchenTbspToCup, "kitchen.tbsp_to_cup", Sig::Fixed(&[R], Some(Real))),
    (KitchenCupToFloz, "kitchen.cup_to_floz", Sig::Fixed(&[R], Some(Real))),
    (KitchenFlozToMl, "kitchen.floz_to_ml", Sig::Fixed(&[R], Some(Real))),
    (KitchenMlToFloz, "kitchen.ml_to_floz", Sig::Fixed(&[R], Some(Real))),
    (KitchenOzToGrams, "kitchen.oz_to_grams", Sig::Fixed(&[R], Some(Real))),
    (KitchenGramsToOz, "kitchen.grams_to_oz", Sig::Fixed(&[R], Some(Real))),
    (KitchenLbToOz, "kitchen.lb_to_oz", Sig::Fixed(&[R], Some(Real))),

    // ======== batch H2 ========
    (ChemMolarity, "chem.molarity", Sig::Fixed(&[R, R], Some(Real))),
    (ChemMoles, "chem.moles", Sig::Fixed(&[R, R], Some(Real))),
    (ChemMass, "chem.mass", Sig::Fixed(&[R, R], Some(Real))),
    (ChemDilution, "chem.dilution", Sig::Fixed(&[R, R, R], Some(Real))),
    (ChemPoh, "chem.poh", Sig::Fixed(&[R], Some(Real))),
    (ChemPercentYield, "chem.percent_yield", Sig::Fixed(&[R, R], Some(Real))),
    (ChemGasPressure, "chem.gas_pressure", Sig::Fixed(&[R, R, R], Some(Real))),
    (ChemDensity, "chem.density", Sig::Fixed(&[R, R], Some(Real))),
    (ChemAtoms, "chem.atoms", Sig::Fixed(&[R], Some(Real))),
    (ChemMolesFromAtoms, "chem.moles_from_atoms", Sig::Fixed(&[R], Some(Real))),
    (ElecPower, "elec.power", Sig::Fixed(&[R, R], Some(Real))),
    (ElecEnergy, "elec.energy", Sig::Fixed(&[R, R], Some(Real))),
    (ElecChargeCap, "elec.capacitor_charge", Sig::Fixed(&[R, R], Some(Real))),
    (ElecInductorEnergy, "elec.inductor_energy", Sig::Fixed(&[R, R], Some(Real))),
    (ElecRcTime, "elec.rc_time", Sig::Fixed(&[R, R], Some(Real))),
    (ElecReactanceC, "elec.reactance_c", Sig::Fixed(&[R, R], Some(Real))),
    (ElecReactanceL, "elec.reactance_l", Sig::Fixed(&[R, R], Some(Real))),
    (ElecResonance, "elec.resonance", Sig::Fixed(&[R, R], Some(Real))),
    (Fin2Cagr, "finance2.cagr", Sig::Fixed(&[R, R, I], Some(Real))),
    (Fin2Pv, "finance2.pv", Sig::Fixed(&[R, R, I], Some(Real))),
    (Fin2Fv, "finance2.fv", Sig::Fixed(&[R, R, I], Some(Real))),
    (Fin2EffectiveRate, "finance2.effective_rate", Sig::Fixed(&[R, I], Some(Real))),
    (Fin2RealReturn, "finance2.real_return", Sig::Fixed(&[R, R], Some(Real))),
    (Fin2DebtRatio, "finance2.debt_ratio", Sig::Fixed(&[R, R], Some(Real))),
    (Fin2CurrentRatio, "finance2.current_ratio", Sig::Fixed(&[R, R], Some(Real))),
    (Fin2Roe, "finance2.roe", Sig::Fixed(&[R, R], Some(Real))),
    (Fin2Eps, "finance2.eps", Sig::Fixed(&[R, R], Some(Real))),
    (Fin2PeRatio, "finance2.pe_ratio", Sig::Fixed(&[R, R], Some(Real))),
    (H2Bsa, "health2.bsa", Sig::Fixed(&[R, R], Some(Real))),
    (H2WaistHip, "health2.waist_hip", Sig::Fixed(&[R, R], Some(Real))),
    (H2TargetHr, "health2.target_hr", Sig::Fixed(&[R, R], Some(Real))),
    (H2CaloriesWalk, "health2.calories_walking", Sig::Fixed(&[R, R], Some(Real))),
    (H2CaloriesRun, "health2.calories_running", Sig::Fixed(&[R, R], Some(Real))),
    (H2Vo2max, "health2.vo2max", Sig::Fixed(&[R, R], Some(Real))),
    (MusicSemitonesBetween, "music.semitones_between", Sig::Fixed(&[R, R], Some(Real))),
    (MusicTranspose, "music.transpose", Sig::Fixed(&[R, R], Some(Real))),
    (MusicBpmToHz, "music.bpm_to_hz", Sig::Fixed(&[R], Some(Real))),
    (MusicHzToBpm, "music.hz_to_bpm", Sig::Fixed(&[R], Some(Real))),
    (MusicCentsToRatio, "music.cents_to_ratio", Sig::Fixed(&[R], Some(Real))),
    (MusicRatioToCents, "music.ratio_to_cents", Sig::Fixed(&[R], Some(Real))),
    (SpaceGravForce, "space.gravity_force", Sig::Fixed(&[R, R, R], Some(Real))),
    (SpaceSchwarzschild, "space.schwarzschild", Sig::Fixed(&[R], Some(Real))),
    (SpaceOrbitalVel, "space.orbital_velocity", Sig::Fixed(&[R, R], Some(Real))),
    (SpaceSurfaceGravity, "space.surface_gravity", Sig::Fixed(&[R, R], Some(Real))),
    (OpticsMagnification, "optics.magnification", Sig::Fixed(&[R, R], Some(Real))),
    (OpticsFocalRadius, "optics.focal_from_radius", Sig::Fixed(&[R], Some(Real))),
    (OpticsLensPower, "optics.power", Sig::Fixed(&[R], Some(Real))),
    (OpticsSnell, "optics.snell", Sig::Fixed(&[R, R, R], Some(Real))),
    (Prob2Binomial, "prob2.binomial_pmf", Sig::Fixed(&[I, I, R], Some(Real))),
    (Prob2Poisson, "prob2.poisson_pmf", Sig::Fixed(&[I, R], Some(Real))),
    (Prob2NormalCdf, "prob2.normal_cdf", Sig::Fixed(&[R], Some(Real))),
    (Prob2NormalPdf, "prob2.normal_pdf", Sig::Fixed(&[R], Some(Real))),
    (RpgDamage, "rpg.damage", Sig::Fixed(&[R, R], Some(Real))),
    (RpgXpForLevel, "rpg.xp_for_level", Sig::Fixed(&[I], Some(Int))),
    (RpgLevelFromXp, "rpg.level_from_xp", Sig::Fixed(&[I], Some(Int))),
    (RpgCrit, "rpg.crit", Sig::Fixed(&[R, R], Some(Real))),
    (RpgHitChance, "rpg.hit_chance", Sig::Fixed(&[R, R], Some(Real))),
    (Phys2EnergyMass, "physics2.energy_mass", Sig::Fixed(&[R], Some(Real))),
    (Phys2PhotonEnergy, "physics2.photon_energy", Sig::Fixed(&[R], Some(Real))),
    (Phys2DeBroglie, "physics2.de_broglie", Sig::Fixed(&[R], Some(Real))),
    (Phys2Coulomb, "physics2.coulomb", Sig::Fixed(&[R, R, R], Some(Real))),
    (Stats2Cv, "stats2.coefficient_variation", Sig::Fixed(&[R, R], Some(Real))),
    (Stats2Se, "stats2.standard_error", Sig::Fixed(&[R, I], Some(Real))),
    (Stats2Margin, "stats2.margin_error", Sig::Fixed(&[R, I, R], Some(Real))),
    (MapMercatorY, "map.mercator_y", Sig::Fixed(&[R], Some(Real))),
    (MapTileX, "map.tile_x", Sig::Fixed(&[R, I], Some(Int))),
    (MapTileY, "map.tile_y", Sig::Fixed(&[R, I], Some(Int))),
    (Date2DayOfYear, "date2.day_of_year", Sig::Fixed(&[I, I, I], Some(Int))),
    (Date2Zeller, "date2.weekday_num", Sig::Fixed(&[I, I, I], Some(Int))),
    (Date2DaysInYear, "date2.days_in_year", Sig::Fixed(&[I], Some(Int))),
    (Wave2Angular, "wave2.angular", Sig::Fixed(&[R], Some(Real))),
    (Wave2Period, "wave2.period", Sig::Fixed(&[R], Some(Real))),
    (Wave2Frequency, "wave2.frequency", Sig::Fixed(&[R], Some(Real))),
    (LightSpeed, "light.speed", Sig::Fixed(&[], Some(Real))),
    (LightTimeToTravel, "light.time_to_travel", Sig::Fixed(&[R], Some(Real))),
    (LightDistanceLy, "light.distance_in_lightyears", Sig::Fixed(&[R], Some(Real))),
    (LightYearsToKm, "light.years_to_km", Sig::Fixed(&[R], Some(Real))),
    (Sound2Speed, "sound2.speed", Sig::Fixed(&[], Some(Real))),
    (Sound2DistFromDelay, "sound2.distance_from_delay", Sig::Fixed(&[R], Some(Real))),
    (Sound2TimeForDist, "sound2.time_for_distance", Sig::Fixed(&[R], Some(Real))),

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
    (GtqRadio, "gtq.radio", Sig::Fixed(&[R, R, S, Bool], Some(Bool))),
    (GtqToggle, "gtq.toggle", Sig::Fixed(&[R, R, S, Bool], Some(Bool))),
    (GtqProgress, "gtq.progress", Sig::Fixed(&[R, R, R, R, R], None)),
    (GtqSeparator, "gtq.separator", Sig::Fixed(&[R, R, R], None)),
    (GtqGroupbox, "gtq.groupbox", Sig::Fixed(&[R, R, R, R, S], None)),
    (GtqSpinbox, "gtq.spinbox", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
    (GtqSelector, "gtq.selector", Sig::Fixed(&[R, R, R, S, I], Some(Int))),
    (GtqTextbox, "gtq.textbox", Sig::Fixed(&[R, R, R, R, S], Some(Str))),
    (GtqVslider, "gtq.vslider", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
    (GtqHovered, "gtq.hovered", Sig::Fixed(&[R, R, R, R], Some(Bool))),
    (GtqClicked, "gtq.clicked", Sig::Fixed(&[R, R, R, R], Some(Bool))),
    // gtq batch G: pure colour helpers
    (GtqLighten, "gtq.lighten", Sig::Fixed(&[S, R], Some(Str))),
    (GtqDarken, "gtq.darken", Sig::Fixed(&[S, R], Some(Str))),
    (GtqMix, "gtq.mix", Sig::Fixed(&[S, S, R], Some(Str))),
    (GtqGrayscale, "gtq.grayscale", Sig::Fixed(&[S], Some(Str))),
    (GtqInvert, "gtq.invert", Sig::Fixed(&[S], Some(Str))),
    (GtqComplement, "gtq.complement", Sig::Fixed(&[S], Some(Str))),
    (GtqTint, "gtq.tint", Sig::Fixed(&[S, R], Some(Str))),
    (GtqShade, "gtq.shade", Sig::Fixed(&[S, R], Some(Str))),
    (GtqContrastColor, "gtq.contrast_color", Sig::Fixed(&[S], Some(Str))),
    (GtqFromRgb, "gtq.from_rgb", Sig::Fixed(&[I, I, I], Some(Str))),
    (GtqFromHsl, "gtq.from_hsl", Sig::Fixed(&[R, R, R], Some(Str))),
    (GtqHueShift, "gtq.hue_shift", Sig::Fixed(&[S, R], Some(Str))),
    (GtqSaturate, "gtq.saturate", Sig::Fixed(&[S, R], Some(Str))),
    (GtqDesaturate, "gtq.desaturate", Sig::Fixed(&[S, R], Some(Str))),
    (GtqWithRed, "gtq.with_red", Sig::Fixed(&[S, I], Some(Str))),
    (GtqWithGreen, "gtq.with_green", Sig::Fixed(&[S, I], Some(Str))),
    (GtqWithBlue, "gtq.with_blue", Sig::Fixed(&[S, I], Some(Str))),
    (GtqBrighten, "gtq.brighten", Sig::Fixed(&[S, R], Some(Str))),
    (GtqDim, "gtq.dim", Sig::Fixed(&[S, R], Some(Str))),
    (GtqMute, "gtq.mute", Sig::Fixed(&[S, R], Some(Str))),
    (GtqRedOf, "gtq.red_of", Sig::Fixed(&[S], Some(Int))),
    (GtqGreenOf, "gtq.green_of", Sig::Fixed(&[S], Some(Int))),
    (GtqBlueOf, "gtq.blue_of", Sig::Fixed(&[S], Some(Int))),
    (GtqLuminanceOf, "gtq.luminance_of", Sig::Fixed(&[S], Some(Real))),
    (GtqIsDark, "gtq.is_dark", Sig::Fixed(&[S], Some(Bool))),
    // gtq batch G: pure layout helpers
    (GtqRowX, "gtq.row_x", Sig::Fixed(&[R, I, R, R], Some(Real))),
    (GtqColY, "gtq.col_y", Sig::Fixed(&[R, I, R, R], Some(Real))),
    (GtqGridX, "gtq.grid_x", Sig::Fixed(&[R, I, R, R], Some(Real))),
    (GtqGridY, "gtq.grid_y", Sig::Fixed(&[R, I, R, R], Some(Real))),
    (GtqCenterX, "gtq.center_x", Sig::Fixed(&[R, R, R], Some(Real))),
    (GtqCenterY, "gtq.center_y", Sig::Fixed(&[R, R, R], Some(Real))),
    (GtqRightX, "gtq.right_x", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GtqBottomY, "gtq.bottom_y", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GtqFitW, "gtq.fit_w", Sig::Fixed(&[R, I, R], Some(Real))),
    (GtqFitH, "gtq.fit_h", Sig::Fixed(&[R, I, R], Some(Real))),
    (GtqAspectW, "gtq.aspect_w", Sig::Fixed(&[R, R], Some(Real))),
    (GtqAspectH, "gtq.aspect_h", Sig::Fixed(&[R, R], Some(Real))),
    (GtqLerpPos, "gtq.lerp_pos", Sig::Fixed(&[R, R, R], Some(Real))),
    (GtqRemap, "gtq.remap", Sig::Fixed(&[R, R, R, R, R], Some(Real))),
    (GtqSnap, "gtq.snap", Sig::Fixed(&[R, R], Some(Real))),
    (GtqGolden, "gtq.golden", Sig::Fixed(&[R], Some(Real))),
    (GtqPadIn, "gtq.pad_in", Sig::Fixed(&[R, R], Some(Real))),
    (GtqInsetW, "gtq.inset_w", Sig::Fixed(&[R, R], Some(Real))),
    (GtqDistribute, "gtq.distribute", Sig::Fixed(&[R, I, I], Some(Real))),
    (GtqSpanEnd, "gtq.span_end", Sig::Fixed(&[R, I, R, R], Some(Real))),
    (GtqScaleAt, "gtq.scale_at", Sig::Fixed(&[R, R, R], Some(Real))),
    (GtqWrapIndex, "gtq.wrap_index", Sig::Fixed(&[I, I], Some(Int))),
    (GtqClampPos, "gtq.clamp_pos", Sig::Fixed(&[R, R, R], Some(Real))),
    (GtqCellX, "gtq.cell_x", Sig::Fixed(&[R, I, R], Some(Real))),
    (GtqCellY, "gtq.cell_y", Sig::Fixed(&[R, I, R], Some(Real))),
    // gtq batch G: vector icons
    (GtqIcon, "gtq.icon", Sig::Fixed(&[R, R, R, S, S], None)),
    (GtqIcCheck, "gtq.ic_check", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcClose, "gtq.ic_close", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcPlus, "gtq.ic_plus", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcMinus, "gtq.ic_minus", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcUp, "gtq.ic_up", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcDown, "gtq.ic_down", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcLeft, "gtq.ic_left", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcRight, "gtq.ic_right", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcChevronUp, "gtq.ic_chevron_up", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcChevronDown, "gtq.ic_chevron_down", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcChevronLeft, "gtq.ic_chevron_left", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcChevronRight, "gtq.ic_chevron_right", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcMenu, "gtq.ic_menu", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcKebab, "gtq.ic_kebab", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcGear, "gtq.ic_gear", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcHome, "gtq.ic_home", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcSearch, "gtq.ic_search", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcHeart, "gtq.ic_heart", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcStar, "gtq.ic_star", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcUser, "gtq.ic_user", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcBell, "gtq.ic_bell", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcMail, "gtq.ic_mail", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcTrash, "gtq.ic_trash", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcPencil, "gtq.ic_pencil", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcSave, "gtq.ic_save", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcFolder, "gtq.ic_folder", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcFile, "gtq.ic_file", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcPlay, "gtq.ic_play", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcPause, "gtq.ic_pause", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcStop, "gtq.ic_stop", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcNext, "gtq.ic_next", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcPrev, "gtq.ic_prev", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcVolume, "gtq.ic_volume", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcMute, "gtq.ic_mute", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcWifi, "gtq.ic_wifi", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcBattery, "gtq.ic_battery", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcLock, "gtq.ic_lock", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcUnlock, "gtq.ic_unlock", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcEye, "gtq.ic_eye", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcEyeOff, "gtq.ic_eye_off", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcCalendar, "gtq.ic_calendar", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcClock, "gtq.ic_clock", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcPin, "gtq.ic_pin", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcTag, "gtq.ic_tag", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcFlag, "gtq.ic_flag", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcBookmark, "gtq.ic_bookmark", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcDownload, "gtq.ic_download", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcUpload, "gtq.ic_upload", Sig::Fixed(&[R, R, R, S], None)),
    (GtqIcRefresh, "gtq.ic_refresh", Sig::Fixed(&[R, R, R, S], None)),
    // gtq batch G: more widgets (display)
    (GtqCard, "gtq.card", Sig::Fixed(&[R, R, R, R, S], None)),
    (GtqCardHeader, "gtq.card_header", Sig::Fixed(&[R, R, R, S], None)),
    (GtqBadge, "gtq.badge", Sig::Fixed(&[R, R, S, S], None)),
    (GtqChip, "gtq.chip", Sig::Fixed(&[R, R, S, S], None)),
    (GtqPill, "gtq.pill", Sig::Fixed(&[R, R, S, S], None)),
    (GtqAvatar, "gtq.avatar", Sig::Fixed(&[R, R, R, S, S], None)),
    (GtqStatusDot, "gtq.status_dot", Sig::Fixed(&[R, R, S], None)),
    (GtqVdivider, "gtq.vdivider", Sig::Fixed(&[R, R, R], None)),
    (GtqTooltip, "gtq.tooltip", Sig::Fixed(&[R, R, S], None)),
    (GtqToast, "gtq.toast", Sig::Fixed(&[R, R, R, S, S], None)),
    (GtqGauge, "gtq.gauge", Sig::Fixed(&[R, R, R, R, R], None)),
    (GtqMeter, "gtq.meter", Sig::Fixed(&[R, R, R, R, R, S], None)),
    (GtqSpinner, "gtq.spinner", Sig::Fixed(&[R, R, R, R], None)),
    (GtqTitlebar, "gtq.titlebar", Sig::Fixed(&[R, R, R, S], None)),
    (GtqStatusbar, "gtq.statusbar", Sig::Fixed(&[R, R, R, S], None)),
    (GtqToolbar, "gtq.toolbar", Sig::Fixed(&[R, R, R, R], None)),
    (GtqBreadcrumb, "gtq.breadcrumb", Sig::Fixed(&[R, R, S], None)),
    (GtqKbd, "gtq.kbd", Sig::Fixed(&[R, R, S], None)),
    (GtqProgressRing, "gtq.progress_ring", Sig::Fixed(&[R, R, R, R, R], None)),
    (GtqSkeleton, "gtq.skeleton", Sig::Fixed(&[R, R, R, R], None)),
    (GtqImagePlaceholder, "gtq.image_placeholder", Sig::Fixed(&[R, R, R, R], None)),
    (GtqRibbon, "gtq.ribbon", Sig::Fixed(&[R, R, S, S], None)),
    (GtqNote, "gtq.note", Sig::Fixed(&[R, R, R, R, S], None)),
    (GtqBar, "gtq.bar", Sig::Fixed(&[R, R, R, R, S], None)),
    // gtq batch G: more widgets (interactive)
    (GtqIconButton, "gtq.icon_button", Sig::Fixed(&[R, R, R, S], Some(Bool))),
    (GtqLink, "gtq.link", Sig::Fixed(&[R, R, S], Some(Bool))),
    (GtqSwitch, "gtq.switch", Sig::Fixed(&[R, R, Bool], Some(Bool))),
    (GtqStepper, "gtq.stepper", Sig::Fixed(&[R, R, I, I, I], Some(Int))),
    (GtqSegmented, "gtq.segmented", Sig::Fixed(&[R, R, R, S, I], Some(Int))),
    (GtqTabs, "gtq.tabs", Sig::Fixed(&[R, R, R, S, I], Some(Int))),
    (GtqNumberbox, "gtq.numberbox", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GtqListItem, "gtq.list_item", Sig::Fixed(&[R, R, R, S, Bool], Some(Bool))),
    (GtqMenuItem, "gtq.menu_item", Sig::Fixed(&[R, R, R, S], Some(Bool))),
    (GtqAccordion, "gtq.accordion", Sig::Fixed(&[R, R, R, S, Bool], Some(Bool))),
    (GtqRating, "gtq.rating", Sig::Fixed(&[R, R, I], Some(Int))),
    (GtqColorSwatch, "gtq.color_swatch", Sig::Fixed(&[R, R, R, S], Some(Bool))),
    (GtqChipClose, "gtq.chip_close", Sig::Fixed(&[R, R, S, S], Some(Bool))),
    (GtqFab, "gtq.fab", Sig::Fixed(&[R, R, R, S], Some(Bool))),
    (GtqPagination, "gtq.pagination", Sig::Fixed(&[R, R, I, I], Some(Int))),
    (GtqButtonPrimary, "gtq.button_primary", Sig::Fixed(&[R, R, R, R, S, S], Some(Bool))),
    (GtqButtonGhost, "gtq.button_ghost", Sig::Fixed(&[R, R, R, R, S], Some(Bool))),
    (GtqKnob, "gtq.knob", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
    (GtqScrollbarV, "gtq.scrollbar_v", Sig::Fixed(&[R, R, R, R, R], Some(Real))),
    (GtqDropdown, "gtq.dropdown", Sig::Fixed(&[R, R, R, S, I], Some(Int))),
    (GtqCheckboxCard, "gtq.checkbox_card", Sig::Fixed(&[R, R, R, S, Bool], Some(Bool))),
    (GtqToggleIcon, "gtq.toggle_icon", Sig::Fixed(&[R, R, R, S, Bool], Some(Bool))),
    (GtqTextButton, "gtq.text_button", Sig::Fixed(&[R, R, S, S], Some(Bool))),
    (GtqSliderMini, "gtq.slider_mini", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GtqStepperF, "gtq.stepper_f", Sig::Fixed(&[R, R, R, R, R, R], Some(Real))),
    (GtqCloseButton, "gtq.close_button", Sig::Fixed(&[R, R, R], Some(Bool))),
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
    "ease", "trig2", "special", "polar", "transform", "temp2", "storage", "area2", "kitchen",
    "chem", "elec", "finance2", "health2", "music", "space", "optics", "prob2", "rpg",
    "physics2", "stats2", "map", "date2", "wave2", "light", "sound2",
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

/// Draw a named vector icon inside the box (x, y)..(x+s, y+s). Used by the `gtq` icon helpers.
fn draw_icon(c: &mut crate::canvas::Canvas, name: &str, x: f64, y: f64, s: f64, col: u32) {
    let l = |c: &mut crate::canvas::Canvas, x1: f64, y1: f64, x2: f64, y2: f64| {
        c.line(x + x1 * s, y + y1 * s, x + x2 * s, y + y2 * s, col);
    };
    let ci = |c: &mut crate::canvas::Canvas, cx: f64, cy: f64, r: f64| {
        c.circle(x + cx * s, y + cy * s, r * s, col);
    };
    let ring = |c: &mut crate::canvas::Canvas, cx: f64, cy: f64, r: f64| {
        let n = 36;
        let (ox, oy) = (x + cx * s, y + cy * s);
        let rr = r * s;
        let mut prev = (ox + rr, oy);
        for k in 1..=n {
            let a = k as f64 / n as f64 * std::f64::consts::TAU;
            let p = (ox + rr * a.cos(), oy + rr * a.sin());
            c.line(prev.0, prev.1, p.0, p.1, col);
            prev = p;
        }
    };
    let rc = |c: &mut crate::canvas::Canvas, x1: f64, y1: f64, x2: f64, y2: f64| {
        c.rect(x + x1 * s, y + y1 * s, (x2 - x1) * s, (y2 - y1) * s, col);
    };
    match name {
        "check" => {
            l(c, 0.2, 0.55, 0.42, 0.78);
            l(c, 0.42, 0.78, 0.82, 0.25);
        }
        "close" => {
            l(c, 0.22, 0.22, 0.78, 0.78);
            l(c, 0.78, 0.22, 0.22, 0.78);
        }
        "plus" => {
            l(c, 0.5, 0.18, 0.5, 0.82);
            l(c, 0.18, 0.5, 0.82, 0.5);
        }
        "minus" => l(c, 0.18, 0.5, 0.82, 0.5),
        "up" => {
            l(c, 0.5, 0.82, 0.5, 0.22);
            l(c, 0.5, 0.22, 0.3, 0.44);
            l(c, 0.5, 0.22, 0.7, 0.44);
        }
        "down" => {
            l(c, 0.5, 0.18, 0.5, 0.78);
            l(c, 0.5, 0.78, 0.3, 0.56);
            l(c, 0.5, 0.78, 0.7, 0.56);
        }
        "left" => {
            l(c, 0.82, 0.5, 0.22, 0.5);
            l(c, 0.22, 0.5, 0.44, 0.3);
            l(c, 0.22, 0.5, 0.44, 0.7);
        }
        "right" => {
            l(c, 0.18, 0.5, 0.78, 0.5);
            l(c, 0.78, 0.5, 0.56, 0.3);
            l(c, 0.78, 0.5, 0.56, 0.7);
        }
        "chevron_up" => {
            l(c, 0.25, 0.6, 0.5, 0.35);
            l(c, 0.5, 0.35, 0.75, 0.6);
        }
        "chevron_down" => {
            l(c, 0.25, 0.4, 0.5, 0.65);
            l(c, 0.5, 0.65, 0.75, 0.4);
        }
        "chevron_left" => {
            l(c, 0.6, 0.25, 0.35, 0.5);
            l(c, 0.35, 0.5, 0.6, 0.75);
        }
        "chevron_right" => {
            l(c, 0.4, 0.25, 0.65, 0.5);
            l(c, 0.65, 0.5, 0.4, 0.75);
        }
        "menu" => {
            l(c, 0.2, 0.3, 0.8, 0.3);
            l(c, 0.2, 0.5, 0.8, 0.5);
            l(c, 0.2, 0.7, 0.8, 0.7);
        }
        "kebab" => {
            ci(c, 0.5, 0.25, 0.06);
            ci(c, 0.5, 0.5, 0.06);
            ci(c, 0.5, 0.75, 0.06);
        }
        "gear" => {
            ring(c, 0.5, 0.5, 0.3);
            ring(c, 0.5, 0.5, 0.12);
            l(c, 0.5, 0.1, 0.5, 0.25);
            l(c, 0.5, 0.75, 0.5, 0.9);
            l(c, 0.1, 0.5, 0.25, 0.5);
            l(c, 0.75, 0.5, 0.9, 0.5);
        }
        "home" => {
            l(c, 0.18, 0.5, 0.5, 0.2);
            l(c, 0.5, 0.2, 0.82, 0.5);
            rc(c, 0.3, 0.5, 0.7, 0.8);
        }
        "search" => {
            ring(c, 0.42, 0.42, 0.22);
            l(c, 0.58, 0.58, 0.82, 0.82);
        }
        "heart" => {
            ci(c, 0.34, 0.38, 0.16);
            ci(c, 0.66, 0.38, 0.16);
            l(c, 0.2, 0.46, 0.5, 0.82);
            l(c, 0.8, 0.46, 0.5, 0.82);
        }
        "star" => {
            let (cx, cy, ro, ri) = (x + 0.5 * s, y + 0.52 * s, 0.4 * s, 0.17 * s);
            let mut pts = [(0.0, 0.0); 10];
            for (k, p) in pts.iter_mut().enumerate() {
                let ang = -std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::PI / 5.0;
                let r = if k % 2 == 0 { ro } else { ri };
                *p = (cx + r * ang.cos(), cy + r * ang.sin());
            }
            for k in 0..10 {
                let (a, b) = (pts[k], pts[(k + 1) % 10]);
                c.line(a.0, a.1, b.0, b.1, col);
            }
        }
        "user" => {
            ci(c, 0.5, 0.35, 0.16);
            l(c, 0.25, 0.82, 0.34, 0.62);
            l(c, 0.34, 0.62, 0.66, 0.62);
            l(c, 0.66, 0.62, 0.75, 0.82);
        }
        "bell" => {
            ring(c, 0.5, 0.42, 0.22);
            l(c, 0.28, 0.6, 0.72, 0.6);
            ci(c, 0.5, 0.72, 0.05);
        }
        "mail" => {
            rc(c, 0.2, 0.3, 0.8, 0.7);
            l(c, 0.2, 0.3, 0.5, 0.52);
            l(c, 0.5, 0.52, 0.8, 0.3);
        }
        "trash" => {
            l(c, 0.25, 0.3, 0.75, 0.3);
            l(c, 0.4, 0.3, 0.42, 0.22);
            l(c, 0.42, 0.22, 0.58, 0.22);
            l(c, 0.58, 0.22, 0.6, 0.3);
            rc(c, 0.3, 0.3, 0.7, 0.8);
        }
        "pencil" => {
            l(c, 0.25, 0.78, 0.68, 0.32);
            l(c, 0.68, 0.32, 0.78, 0.42);
            l(c, 0.78, 0.42, 0.35, 0.88);
            l(c, 0.25, 0.78, 0.22, 0.88);
        }
        "save" => {
            rc(c, 0.22, 0.22, 0.78, 0.78);
            rc(c, 0.35, 0.22, 0.65, 0.42);
            rc(c, 0.32, 0.55, 0.68, 0.78);
        }
        "folder" => {
            rc(c, 0.2, 0.35, 0.8, 0.75);
            l(c, 0.2, 0.35, 0.34, 0.28);
            l(c, 0.34, 0.28, 0.48, 0.35);
        }
        "file" => {
            rc(c, 0.3, 0.2, 0.7, 0.8);
            l(c, 0.58, 0.2, 0.58, 0.32);
            l(c, 0.58, 0.32, 0.7, 0.32);
        }
        "play" => {
            l(c, 0.32, 0.25, 0.32, 0.75);
            l(c, 0.32, 0.25, 0.78, 0.5);
            l(c, 0.32, 0.75, 0.78, 0.5);
        }
        "pause" => {
            rc(c, 0.32, 0.25, 0.44, 0.75);
            rc(c, 0.56, 0.25, 0.68, 0.75);
        }
        "stop" => rc(c, 0.3, 0.3, 0.7, 0.7),
        "next" => {
            l(c, 0.28, 0.25, 0.28, 0.75);
            l(c, 0.28, 0.25, 0.62, 0.5);
            l(c, 0.28, 0.75, 0.62, 0.5);
            l(c, 0.68, 0.25, 0.68, 0.75);
        }
        "prev" => {
            l(c, 0.72, 0.25, 0.72, 0.75);
            l(c, 0.72, 0.25, 0.38, 0.5);
            l(c, 0.72, 0.75, 0.38, 0.5);
            l(c, 0.32, 0.25, 0.32, 0.75);
        }
        "volume" => {
            rc(c, 0.2, 0.42, 0.35, 0.58);
            l(c, 0.35, 0.5, 0.55, 0.3);
            l(c, 0.55, 0.3, 0.55, 0.7);
            l(c, 0.35, 0.5, 0.55, 0.7);
            l(c, 0.64, 0.4, 0.72, 0.5);
            l(c, 0.72, 0.5, 0.64, 0.6);
        }
        "mute" => {
            rc(c, 0.2, 0.42, 0.35, 0.58);
            l(c, 0.35, 0.5, 0.55, 0.3);
            l(c, 0.55, 0.3, 0.55, 0.7);
            l(c, 0.35, 0.5, 0.55, 0.7);
            l(c, 0.62, 0.4, 0.8, 0.6);
            l(c, 0.8, 0.4, 0.62, 0.6);
        }
        "wifi" => {
            ci(c, 0.5, 0.78, 0.05);
            ring(c, 0.5, 0.78, 0.22);
            ring(c, 0.5, 0.78, 0.38);
        }
        "battery" => {
            rc(c, 0.2, 0.38, 0.75, 0.62);
            rc(c, 0.75, 0.45, 0.8, 0.55);
            rc(c, 0.24, 0.42, 0.5, 0.58);
        }
        "lock" => {
            rc(c, 0.3, 0.5, 0.7, 0.8);
            ring(c, 0.5, 0.42, 0.14);
            l(c, 0.36, 0.42, 0.36, 0.5);
            l(c, 0.64, 0.42, 0.64, 0.5);
        }
        "unlock" => {
            rc(c, 0.3, 0.5, 0.7, 0.8);
            ring(c, 0.38, 0.42, 0.14);
            l(c, 0.52, 0.4, 0.52, 0.5);
        }
        "eye" => {
            l(c, 0.2, 0.5, 0.5, 0.32);
            l(c, 0.5, 0.32, 0.8, 0.5);
            l(c, 0.2, 0.5, 0.5, 0.68);
            l(c, 0.5, 0.68, 0.8, 0.5);
            ci(c, 0.5, 0.5, 0.1);
        }
        "eye_off" => {
            l(c, 0.2, 0.5, 0.5, 0.32);
            l(c, 0.5, 0.32, 0.8, 0.5);
            l(c, 0.2, 0.5, 0.5, 0.68);
            l(c, 0.5, 0.68, 0.8, 0.5);
            ci(c, 0.5, 0.5, 0.1);
            l(c, 0.2, 0.8, 0.8, 0.2);
        }
        "calendar" => {
            rc(c, 0.2, 0.28, 0.8, 0.78);
            l(c, 0.2, 0.42, 0.8, 0.42);
            l(c, 0.35, 0.28, 0.35, 0.2);
            l(c, 0.65, 0.28, 0.65, 0.2);
        }
        "clock" => {
            ring(c, 0.5, 0.5, 0.3);
            l(c, 0.5, 0.5, 0.5, 0.3);
            l(c, 0.5, 0.5, 0.64, 0.5);
        }
        "pin" => {
            ci(c, 0.5, 0.4, 0.18);
            l(c, 0.36, 0.5, 0.5, 0.82);
            l(c, 0.64, 0.5, 0.5, 0.82);
        }
        "tag" => {
            l(c, 0.25, 0.3, 0.6, 0.3);
            l(c, 0.6, 0.3, 0.8, 0.5);
            l(c, 0.8, 0.5, 0.6, 0.7);
            l(c, 0.6, 0.7, 0.25, 0.7);
            l(c, 0.25, 0.7, 0.25, 0.3);
            ci(c, 0.37, 0.44, 0.05);
        }
        "flag" => {
            l(c, 0.3, 0.2, 0.3, 0.8);
            rc(c, 0.3, 0.24, 0.66, 0.46);
        }
        "bookmark" => {
            l(c, 0.32, 0.2, 0.32, 0.8);
            l(c, 0.32, 0.8, 0.5, 0.62);
            l(c, 0.5, 0.62, 0.68, 0.8);
            l(c, 0.68, 0.8, 0.68, 0.2);
            l(c, 0.32, 0.2, 0.68, 0.2);
        }
        "download" => {
            l(c, 0.5, 0.22, 0.5, 0.62);
            l(c, 0.35, 0.48, 0.5, 0.64);
            l(c, 0.65, 0.48, 0.5, 0.64);
            l(c, 0.28, 0.78, 0.72, 0.78);
        }
        "upload" => {
            l(c, 0.5, 0.64, 0.5, 0.24);
            l(c, 0.35, 0.4, 0.5, 0.24);
            l(c, 0.65, 0.4, 0.5, 0.24);
            l(c, 0.28, 0.78, 0.72, 0.78);
        }
        "refresh" => {
            ring(c, 0.5, 0.5, 0.28);
            l(c, 0.78, 0.5, 0.68, 0.38);
            l(c, 0.78, 0.5, 0.88, 0.38);
        }
        _ => ci(c, 0.5, 0.5, 0.3),
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
            GtqRadio => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let selected = matches!(&args[3], Value::Bool(true));
                let r = 8.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    (((mx as f64 - (x + r)).powi(2) + (my as f64 - (y + r)).powi(2)).sqrt() <= r + 2.0, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let (outer, fill, dot, white) =
                    (parse_color("#888888")?, parse_color("#374151")?, parse_color("#60a5fa")?, parse_color("#dddddd")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.circle(x + r, y + r, r, outer);
                g.canvas.circle(x + r, y + r, r - 1.5, fill);
                if selected {
                    g.canvas.circle(x + r, y + r, r - 4.0, dot);
                }
                self.fonts.draw(&mut g.canvas, x + 2.0 * r + 8.0, y + 2.0, &label, white)?;
                Value::Bool(clicked)
            }
            GtqToggle => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let on = matches!(&args[3], Value::Bool(true));
                let (w, h) = (40.0, 18.0);
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let new_on = if hover && down && !self.prev_down { !on } else { on };
                let (track, knob, white) =
                    (parse_color(if new_on { "#22c55e" } else { "#4a5568" })?, parse_color("#f8fafc")?, parse_color("#dddddd")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, track);
                let kx = if new_on { x + w - h / 2.0 - 2.0 } else { x + h / 2.0 + 2.0 };
                g.canvas.circle(kx, y + h / 2.0, h / 2.0 - 2.0, knob);
                self.fonts.draw(&mut g.canvas, x + w + 8.0, y + 1.0, &label, white)?;
                Value::Bool(new_on)
            }
            GtqProgress => {
                let (x, y, w, val, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
                let h = 14.0;
                let t = if maxv > 0.0 { (val / maxv).clamp(0.0, 1.0) } else { 0.0 };
                let (bg, fg) = (parse_color("#374151")?, parse_color("#60a5fa")?);
                let g = self.window()?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.rect(x, y, w * t, h, fg);
                Value::Null
            }
            GtqSeparator => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let c = parse_color("#475569")?;
                self.window()?.canvas.line(x, y, x + w, y, c);
                Value::Null
            }
            GtqGroupbox => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let title = text(&args[4]).to_string();
                let (border, white) = (parse_color("#475569")?, parse_color("#94a3b8")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.line(x, y, x + w, y, border);
                g.canvas.line(x, y + h, x + w, y + h, border);
                g.canvas.line(x, y, x, y + h, border);
                g.canvas.line(x + w, y, x + w, y + h, border);
                self.fonts.draw(&mut g.canvas, x + 8.0, y - 7.0, &title, white)?;
                Value::Null
            }
            GtqSpinbox => {
                let (x, y, val, step, minv, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]), real(&args[5]));
                let (bw, fw, h) = (22.0, 60.0, 24.0);
                let (mx, my, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let edge = down && !self.prev_down;
                let over = |bx: f64| mx >= bx && mx <= bx + bw && my >= y && my <= y + h;
                let mut v = val;
                if edge && over(x) {
                    v = (v - step).max(minv);
                }
                if edge && over(x + bw + fw) {
                    v = (v + step).min(maxv);
                }
                let (btn, box_c, white) = (parse_color("#4a5568")?, parse_color("#1f2937")?, parse_color("#e2e8f0")?);
                let txt = crate::vm::format_real(v);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, bw, h, btn);
                g.canvas.rect(x + bw, y, fw, h, box_c);
                g.canvas.rect(x + bw + fw, y, bw, h, btn);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 4.0, "-", white)?;
                self.fonts.draw(&mut g.canvas, x + bw + fw + 7.0, y + 4.0, "+", white)?;
                self.fonts.draw(&mut g.canvas, x + bw + 8.0, y + 4.0, &txt, white)?;
                Value::Real(v)
            }
            GtqSelector => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let opts: Vec<String> = text(&args[3]).split(',').map(|s| s.trim().to_string()).collect();
                let n = opts.len().max(1);
                let idx = int(&args[4]).rem_euclid(n as i64) as usize;
                let h = 24.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    let (a, b) = (a as f64, b as f64);
                    (a >= x && a <= x + w && b >= y && b <= y + h, g.click())
                };
                let new_idx = if hover && down && !self.prev_down { (idx + 1) % n } else { idx };
                let (bg, white) = (parse_color(if hover { "#4a5568" } else { "#374151" })?, parse_color("#e2e8f0")?);
                let shown = opts.get(new_idx).cloned().unwrap_or_default();
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 4.0, &shown, white)?;
                self.fonts.draw(&mut g.canvas, x + w - 14.0, y + 4.0, ">", white)?;
                Value::Int(new_idx as i64)
            }
            GtqTextbox => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let current = text(&args[4]).to_string();
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    let (a, b) = (a as f64, b as f64);
                    (a >= x && a <= x + w && b >= y && b <= y + h, g.click())
                };
                let mut value = current.clone();
                if hover && down && !self.prev_down {
                    if let Ok(s) = dialogs::ask(&format!("New value (was: {})", current)) {
                        if !s.is_empty() {
                            value = s;
                        }
                    }
                }
                let (bg, border, white) =
                    (parse_color("#1f2937")?, parse_color(if hover { "#60a5fa" } else { "#475569" })?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x, y, x + w, y, border);
                g.canvas.line(x, y + h, x + w, y + h, border);
                g.canvas.line(x, y, x, y + h, border);
                g.canvas.line(x + w, y, x + w, y + h, border);
                self.fonts.draw(&mut g.canvas, x + 6.0, y + h / 2.0 - 6.0, &value, white)?;
                Value::Str(value.into())
            }
            GtqVslider => {
                let (x, y, h) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let (val, lo, hi) = (real(&args[3]), real(&args[4]), real(&args[5]));
                let (hover, down, myf) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    let (a, b) = (a as f64, b as f64);
                    (a >= x - 8.0 && a <= x + 8.0 && b >= y && b <= y + h, g.click(), b)
                };
                let mut v = val;
                if hover && down && hi > lo && h > 0.0 {
                    v = hi - ((myf - y) / h).clamp(0.0, 1.0) * (hi - lo);
                }
                let (track, knob) = (parse_color("#374151")?, parse_color("#60a5fa")?);
                let t = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x - 2.0, y, 4.0, h, track);
                g.canvas.circle(x, y + (1.0 - t) * h, 8.0, knob);
                Value::Real(v)
            }
            GtqHovered => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                let (mx, my) = g.mouse();
                let (mx, my) = (mx as f64, my as f64);
                Value::Bool(mx >= x && mx <= x + w && my >= y && my <= y + h)
            }
            GtqClicked => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                Value::Bool(hover && down && !self.prev_down)
            }
            // ---- gtq batch G: display widgets ----
            GtqCard => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let bg = parse_color(text(&args[4]))?;
                self.window()?.canvas.rect(x, y, w, h, bg);
                Value::Null
            }
            GtqCardHeader => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let title = text(&args[3]).to_string();
                let (bg, line, white) = (parse_color("#1e293b")?, parse_color("#334155")?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, 30.0, bg);
                g.canvas.line(x, y + 30.0, x + w, y + 30.0, line);
                self.fonts.draw(&mut g.canvas, x + 10.0, y + 9.0, &title, white)?;
                Value::Null
            }
            GtqBadge => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let (bg, white) = (parse_color(text(&args[3]))?, parse_color("#ffffff")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, tw + 16.0, 20.0, bg);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 4.0, &label, white)?;
                Value::Null
            }
            GtqChip => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let (fg, bg) = (parse_color(text(&args[3]))?, parse_color("#1e293b")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (w, h) = (tw + 18.0, 22.0);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x, y, x + w, y, fg);
                g.canvas.line(x, y + h, x + w, y + h, fg);
                g.canvas.line(x, y, x, y + h, fg);
                g.canvas.line(x + w, y, x + w, y + h, fg);
                self.fonts.draw(&mut g.canvas, x + 9.0, y + 5.0, &label, fg)?;
                Value::Null
            }
            GtqPill => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let (bg, white) = (parse_color(text(&args[3]))?, parse_color("#ffffff")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (w, h) = (tw + 20.0, 22.0);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x + h / 2.0, y, w - h, h, bg);
                g.canvas.circle(x + h / 2.0, y + h / 2.0, h / 2.0, bg);
                g.canvas.circle(x + w - h / 2.0, y + h / 2.0, h / 2.0, bg);
                self.fonts.draw(&mut g.canvas, x + 10.0, y + 5.0, &label, white)?;
                Value::Null
            }
            GtqAvatar => {
                let (x, y, size) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let initial = text(&args[3]).to_string();
                let (bg, white) = (parse_color(text(&args[4]))?, parse_color("#ffffff")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.circle(x + size / 2.0, y + size / 2.0, size / 2.0, bg);
                self.fonts.draw(&mut g.canvas, x + size / 2.0 - 4.0, y + size / 2.0 - 6.0, &initial, white)?;
                Value::Null
            }
            GtqStatusDot => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let c = parse_color(text(&args[2]))?;
                self.window()?.canvas.circle(x + 5.0, y + 5.0, 5.0, c);
                Value::Null
            }
            GtqVdivider => {
                let (x, y, h) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let c = parse_color("#475569")?;
                self.window()?.canvas.line(x, y, x, y + h, c);
                Value::Null
            }
            GtqTooltip => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let (bg, white) = (parse_color("#0b1220")?, parse_color("#e2e8f0")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, tw + 16.0, 24.0, bg);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 6.0, &label, white)?;
                Value::Null
            }
            GtqToast => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let label = text(&args[3]).to_string();
                let (accent, bg, white) =
                    (parse_color(text(&args[4]))?, parse_color("#1e293b")?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, 40.0, bg);
                g.canvas.rect(x, y, 5.0, 40.0, accent);
                self.fonts.draw(&mut g.canvas, x + 16.0, y + 14.0, &label, white)?;
                Value::Null
            }
            GtqGauge => {
                let (x, y, r, val, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
                let t = if maxv > 0.0 { (val / maxv).clamp(0.0, 1.0) } else { 0.0 };
                let (track, fg) = (parse_color("#374151")?, parse_color("#60a5fa")?);
                let (cx, cy) = (x + r, y + r);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                let n = 40;
                for k in 0..n {
                    let a0 = std::f64::consts::PI + k as f64 / n as f64 * std::f64::consts::PI;
                    let a1 = std::f64::consts::PI + (k + 1) as f64 / n as f64 * std::f64::consts::PI;
                    let col = if (k as f64 / n as f64) < t { fg } else { track };
                    g.canvas.line(cx + r * a0.cos(), cy + r * a0.sin(), cx + r * a1.cos(), cy + r * a1.sin(), col);
                }
                Value::Null
            }
            GtqMeter => {
                let (x, y, w, val, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
                let fg = parse_color(text(&args[5]))?;
                let bg = parse_color("#374151")?;
                let t = if maxv > 0.0 { (val / maxv).clamp(0.0, 1.0) } else { 0.0 };
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, 12.0, bg);
                g.canvas.rect(x, y, w * t, 12.0, fg);
                Value::Null
            }
            GtqSpinner => {
                let (x, y, r, phase) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let (track, fg) = (parse_color("#334155")?, parse_color("#60a5fa")?);
                let (cx, cy) = (x + r, y + r);
                let start = phase * std::f64::consts::TAU;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                let n = 32;
                for k in 0..n {
                    let a0 = k as f64 / n as f64 * std::f64::consts::TAU;
                    let a1 = (k + 1) as f64 / n as f64 * std::f64::consts::TAU;
                    let rel = (a0 - start).rem_euclid(std::f64::consts::TAU);
                    let col = if rel < 0.3 * std::f64::consts::TAU { fg } else { track };
                    g.canvas.line(cx + r * a0.cos(), cy + r * a0.sin(), cx + r * a1.cos(), cy + r * a1.sin(), col);
                }
                Value::Null
            }
            GtqTitlebar => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let title = text(&args[3]).to_string();
                let (bg, white) = (parse_color("#0b1220")?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, 34.0, bg);
                self.fonts.draw(&mut g.canvas, x + 12.0, y + 11.0, &title, white)?;
                Value::Null
            }
            GtqStatusbar => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let txt = text(&args[3]).to_string();
                let (bg, gray) = (parse_color("#1e293b")?, parse_color("#94a3b8")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, 24.0, bg);
                self.fonts.draw(&mut g.canvas, x + 10.0, y + 6.0, &txt, gray)?;
                Value::Null
            }
            GtqToolbar => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let (bg, line) = (parse_color("#1e293b")?, parse_color("#334155")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x, y + h, x + w, y + h, line);
                Value::Null
            }
            GtqBreadcrumb => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let txt = text(&args[2]).to_string();
                let gray = parse_color("#94a3b8")?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                self.fonts.draw(&mut g.canvas, x, y, &txt, gray)?;
                Value::Null
            }
            GtqKbd => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let (bg, border, white) =
                    (parse_color("#334155")?, parse_color("#64748b")?, parse_color("#e2e8f0")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (w, h) = (tw + 14.0, 22.0);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x, y, x + w, y, border);
                g.canvas.line(x, y + h, x + w, y + h, border);
                g.canvas.line(x, y, x, y + h, border);
                g.canvas.line(x + w, y, x + w, y + h, border);
                self.fonts.draw(&mut g.canvas, x + 7.0, y + 5.0, &label, white)?;
                Value::Null
            }
            GtqProgressRing => {
                let (x, y, r, val, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
                let t = if maxv > 0.0 { (val / maxv).clamp(0.0, 1.0) } else { 0.0 };
                let (track, fg) = (parse_color("#374151")?, parse_color("#60a5fa")?);
                let (cx, cy) = (x + r, y + r);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                let n = 48;
                for k in 0..n {
                    let base = -std::f64::consts::FRAC_PI_2;
                    let a0 = base + k as f64 / n as f64 * std::f64::consts::TAU;
                    let a1 = base + (k + 1) as f64 / n as f64 * std::f64::consts::TAU;
                    let col = if (k as f64 / n as f64) < t { fg } else { track };
                    g.canvas.line(cx + r * a0.cos(), cy + r * a0.sin(), cx + r * a1.cos(), cy + r * a1.sin(), col);
                }
                Value::Null
            }
            GtqSkeleton => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let c = parse_color("#334155")?;
                self.window()?.canvas.rect(x, y, w, h, c);
                Value::Null
            }
            GtqImagePlaceholder => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let (bg, line) = (parse_color("#1e293b")?, parse_color("#475569")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x, y, x + w, y, line);
                g.canvas.line(x, y + h, x + w, y + h, line);
                g.canvas.line(x, y, x, y + h, line);
                g.canvas.line(x + w, y, x + w, y + h, line);
                g.canvas.line(x, y, x + w, y + h, line);
                g.canvas.line(x + w, y, x, y + h, line);
                Value::Null
            }
            GtqRibbon => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let (bg, white) = (parse_color(text(&args[3]))?, parse_color("#ffffff")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (w, h) = (tw + 16.0, 22.0);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x + w, y, x + w + 8.0, y + h / 2.0, bg);
                g.canvas.line(x + w + 8.0, y + h / 2.0, x + w, y + h, bg);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 5.0, &label, white)?;
                Value::Null
            }
            GtqNote => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let (bg, fold) = (parse_color(text(&args[4]))?, parse_color("#1f2937")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x + w - 14.0, y + h, x + w, y + h - 14.0, fold);
                Value::Null
            }
            GtqBar => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let c = parse_color(text(&args[4]))?;
                self.window()?.canvas.rect(x, y, w, h, c);
                Value::Null
            }

            // ---- gtq batch G: interactive widgets ----
            GtqIconButton => {
                let (x, y, size) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let icon = text(&args[3]).to_string();
                let bx = size + 12.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + bx && my >= y && my <= y + bx, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let (bg, fg) = (parse_color(if hover { "#334155" } else { "#1e293b" })?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, bx, bx, bg);
                draw_icon(&mut g.canvas, &icon, x + 6.0, y + 6.0, size, fg);
                Value::Bool(clicked)
            }
            GtqLink => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + tw && my >= y && my <= y + 16.0, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let col = parse_color(if hover { "#93c5fd" } else { "#60a5fa" })?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                self.fonts.draw(&mut g.canvas, x, y, &label, col)?;
                g.canvas.line(x, y + 15.0, x + tw, y + 15.0, col);
                Value::Bool(clicked)
            }
            GtqSwitch => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let on = matches!(&args[2], Value::Bool(true));
                let (w, h) = (40.0, 18.0);
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let new_on = if hover && down && !self.prev_down { !on } else { on };
                let (track, knob) =
                    (parse_color(if new_on { "#22c55e" } else { "#4a5568" })?, parse_color("#f8fafc")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, track);
                let kx = if new_on { x + w - h / 2.0 - 2.0 } else { x + h / 2.0 + 2.0 };
                g.canvas.circle(kx, y + h / 2.0, h / 2.0 - 2.0, knob);
                Value::Bool(new_on)
            }
            GtqStepper => {
                let (x, y, val, minv, maxv) =
                    (real(&args[0]), real(&args[1]), int(&args[2]), int(&args[3]), int(&args[4]));
                let (bw, fw, h) = (22.0, 44.0, 24.0);
                let (mx, my, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let edge = down && !self.prev_down;
                let over = |bx: f64| mx >= bx && mx <= bx + bw && my >= y && my <= y + h;
                let mut v = val;
                if edge && over(x) {
                    v = (v - 1).max(minv);
                }
                if edge && over(x + bw + fw) {
                    v = (v + 1).min(maxv);
                }
                let (btn, box_c, white) =
                    (parse_color("#4a5568")?, parse_color("#1f2937")?, parse_color("#e2e8f0")?);
                let txt = v.to_string();
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, bw, h, btn);
                g.canvas.rect(x + bw, y, fw, h, box_c);
                g.canvas.rect(x + bw + fw, y, bw, h, btn);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 4.0, "-", white)?;
                self.fonts.draw(&mut g.canvas, x + bw + fw + 7.0, y + 4.0, "+", white)?;
                self.fonts.draw(&mut g.canvas, x + bw + 10.0, y + 4.0, &txt, white)?;
                Value::Int(v)
            }
            GtqSegmented | GtqTabs => {
                let is_tabs = matches!(b, GtqTabs);
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let opts: Vec<String> = text(&args[3]).split(',').map(|s| s.trim().to_string()).collect();
                let n = opts.len().max(1);
                let idx = int(&args[4]).rem_euclid(n as i64) as usize;
                let h = if is_tabs { 30.0 } else { 28.0 };
                let seg = w / n as f64;
                let (mx, my, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let edge = down && !self.prev_down;
                let mut new_idx = idx;
                if edge && my >= y && my <= y + h && mx >= x && mx <= x + w {
                    new_idx = (((mx - x) / seg).floor() as usize).min(n - 1);
                }
                let widths: Vec<f64> = opts.iter().map(|o| self.fonts.width(o).unwrap_or(0) as f64).collect();
                let (bg, sel, white, dim, line) = (
                    parse_color("#1e293b")?,
                    parse_color("#3b82f6")?,
                    parse_color("#e2e8f0")?,
                    parse_color("#94a3b8")?,
                    parse_color("#334155")?,
                );
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                if is_tabs {
                    g.canvas.line(x, y + h, x + w, y + h, line);
                } else {
                    g.canvas.rect(x, y, w, h, bg);
                }
                for i in 0..n {
                    let sx = x + i as f64 * seg;
                    if is_tabs {
                        if i == new_idx {
                            g.canvas.rect(sx + 8.0, y + h - 3.0, seg - 16.0, 3.0, sel);
                        }
                    } else {
                        if i == new_idx {
                            g.canvas.rect(sx, y, seg, h, sel);
                        }
                        if i > 0 {
                            g.canvas.line(sx, y, sx, y + h, line);
                        }
                    }
                }
                for i in 0..n {
                    let sx = x + i as f64 * seg;
                    let col = if is_tabs && i != new_idx { dim } else { white };
                    self.fonts.draw(&mut g.canvas, sx + (seg - widths[i]) / 2.0, y + 7.0, &opts[i], col)?;
                }
                Value::Int(new_idx as i64)
            }
            GtqNumberbox => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let cur = real(&args[3]);
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + 26.0, g.click())
                };
                let mut v = cur;
                if hover && down && !self.prev_down {
                    if let Ok(s) = dialogs::ask(&format!("New number (was {})", crate::vm::format_real(cur))) {
                        if let Ok(parsed) = s.trim().replace(',', ".").parse::<f64>() {
                            v = parsed;
                        }
                    }
                }
                let (bg, border, white) = (
                    parse_color("#1f2937")?,
                    parse_color(if hover { "#60a5fa" } else { "#475569" })?,
                    parse_color("#e2e8f0")?,
                );
                let txt = crate::vm::format_real(v);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, 26.0, bg);
                g.canvas.line(x, y, x + w, y, border);
                g.canvas.line(x, y + 26.0, x + w, y + 26.0, border);
                g.canvas.line(x, y, x, y + 26.0, border);
                g.canvas.line(x + w, y, x + w, y + 26.0, border);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 7.0, &txt, white)?;
                Value::Real(v)
            }
            GtqListItem => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let label = text(&args[3]).to_string();
                let selected = matches!(&args[4], Value::Bool(true));
                let h = 30.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let bg = parse_color(if selected {
                    "#334155"
                } else if hover {
                    "#1e293b"
                } else {
                    "#0f172a"
                })?;
                let white = parse_color("#e2e8f0")?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                self.fonts.draw(&mut g.canvas, x + 10.0, y + 8.0, &label, white)?;
                Value::Bool(clicked)
            }
            GtqMenuItem => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let label = text(&args[3]).to_string();
                let h = 28.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let (bg, white) = (parse_color(if hover { "#334155" } else { "#1e293b" })?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                self.fonts.draw(&mut g.canvas, x + 10.0, y + 7.0, &label, white)?;
                Value::Bool(clicked)
            }
            GtqAccordion => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let title = text(&args[3]).to_string();
                let open = matches!(&args[4], Value::Bool(true));
                let h = 32.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let new_open = if hover && down && !self.prev_down { !open } else { open };
                let (bg, white) = (parse_color(if hover { "#1e293b" } else { "#172033" })?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                draw_icon(&mut g.canvas, if new_open { "chevron_down" } else { "chevron_right" }, x + 8.0, y + 8.0, 16.0, white);
                self.fonts.draw(&mut g.canvas, x + 32.0, y + 9.0, &title, white)?;
                Value::Bool(new_open)
            }
            GtqRating => {
                let (x, y, val) = (real(&args[0]), real(&args[1]), int(&args[2]));
                let size = 22.0;
                let (mx, my, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let mut v = val;
                if down && !self.prev_down && my >= y && my <= y + size && mx >= x && mx < x + 5.0 * size {
                    v = (((mx - x) / size).floor() as i64 + 1).clamp(1, 5);
                }
                let (on, off) = (parse_color("#fbbf24")?, parse_color("#475569")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                for i in 0..5 {
                    let col = if (i as i64) < v { on } else { off };
                    draw_icon(&mut g.canvas, "star", x + i as f64 * size, y, size - 2.0, col);
                }
                Value::Int(v)
            }
            GtqColorSwatch => {
                let (x, y, size) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + size && my >= y && my <= y + size, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let c = parse_color(text(&args[3]))?;
                let border = parse_color(if hover { "#ffffff" } else { "#475569" })?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, size, size, c);
                g.canvas.line(x, y, x + size, y, border);
                g.canvas.line(x, y + size, x + size, y + size, border);
                g.canvas.line(x, y, x, y + size, border);
                g.canvas.line(x + size, y, x + size, y + size, border);
                Value::Bool(clicked)
            }
            GtqChipClose => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let bg = parse_color(text(&args[3]))?;
                let white = parse_color("#ffffff")?;
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (w, h) = (tw + 34.0, 22.0);
                let (mxf, myf, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let x_clicked =
                    down && !self.prev_down && mxf >= x + w - 22.0 && mxf <= x + w && myf >= y && myf <= y + h;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                self.fonts.draw(&mut g.canvas, x + 9.0, y + 5.0, &label, white)?;
                draw_icon(&mut g.canvas, "close", x + w - 20.0, y + 3.0, 16.0, white);
                Value::Bool(x_clicked)
            }
            GtqFab => {
                let (x, y, r) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let icon = text(&args[3]).to_string();
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (dx, dy) = (mx as f64 - (x + r), my as f64 - (y + r));
                    ((dx * dx + dy * dy).sqrt() <= r, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let (bg, fg) = (parse_color(if hover { "#2563eb" } else { "#3b82f6" })?, parse_color("#ffffff")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.circle(x + r, y + r, r, bg);
                draw_icon(&mut g.canvas, &icon, x + r * 0.5, y + r * 0.5, r, fg);
                Value::Bool(clicked)
            }
            GtqPagination => {
                let (x, y, pages, current) = (real(&args[0]), real(&args[1]), int(&args[2]), int(&args[3]));
                let cell = 28.0;
                let (mx, my, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let mut cur = current;
                if down && !self.prev_down && my >= y && my <= y + cell && mx >= x && mx < x + pages as f64 * cell {
                    cur = (((mx - x) / cell).floor() as i64 + 1).clamp(1, pages.max(1));
                }
                let (sel, bg, white) = (parse_color("#3b82f6")?, parse_color("#1e293b")?, parse_color("#e2e8f0")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                for i in 1..=pages {
                    let sx = x + (i - 1) as f64 * cell;
                    g.canvas.rect(sx, y, cell - 2.0, cell, if i == cur { sel } else { bg });
                }
                for i in 1..=pages {
                    let sx = x + (i - 1) as f64 * cell;
                    let s = i.to_string();
                    let tw = self.fonts.width(&s).unwrap_or(0) as f64;
                    self.fonts.draw(&mut g.canvas, sx + (cell - tw) / 2.0, y + 8.0, &s, white)?;
                }
                Value::Int(cur)
            }
            GtqButtonPrimary => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let label = text(&args[4]).to_string();
                let base = parse_color(text(&args[5]))?;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let pressed = hover && down && !self.prev_down;
                let white = parse_color("#ffffff")?;
                let hi = parse_color("#bfdbfe")?;
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, base);
                if hover {
                    g.canvas.rect(x, y, w, 2.0, hi);
                }
                self.fonts.draw(&mut g.canvas, (x + (w - tw) / 2.0).max(x + 2.0), y + h / 2.0 - 6.0, &label, white)?;
                Value::Bool(pressed)
            }
            GtqButtonGhost => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let label = text(&args[4]).to_string();
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let pressed = hover && down && !self.prev_down;
                let (border, txtc, hoverbg) =
                    (parse_color("#60a5fa")?, parse_color("#93c5fd")?, parse_color("#1e293b")?);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                if hover {
                    g.canvas.rect(x, y, w, h, hoverbg);
                }
                g.canvas.line(x, y, x + w, y, border);
                g.canvas.line(x, y + h, x + w, y + h, border);
                g.canvas.line(x, y, x, y + h, border);
                g.canvas.line(x + w, y, x + w, y + h, border);
                self.fonts.draw(&mut g.canvas, (x + (w - tw) / 2.0).max(x + 2.0), y + h / 2.0 - 6.0, &label, txtc)?;
                Value::Bool(pressed)
            }
            GtqKnob => {
                let (x, y, r, val, lo, hi) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]), real(&args[5]));
                let (cx, cy) = (x + r, y + r);
                let (hover, down, ang) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (dx, dy) = (mx as f64 - cx, my as f64 - cy);
                    ((dx * dx + dy * dy).sqrt() <= r + 4.0, g.click(), dy.atan2(dx))
                };
                let mut v = val;
                if hover && down && hi > lo {
                    let mut d = (ang.to_degrees() - 135.0).rem_euclid(360.0);
                    if d > 270.0 {
                        d = if d > 315.0 { 0.0 } else { 270.0 };
                    }
                    v = lo + (d / 270.0) * (hi - lo);
                }
                let t = if hi > lo { ((v - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
                let (body, fg) = (parse_color("#1e293b")?, parse_color("#60a5fa")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.circle(cx, cy, r, body);
                let ind = (135.0 + t * 270.0).to_radians();
                g.canvas.line(cx, cy, cx + r * 0.8 * ind.cos(), cy + r * 0.8 * ind.sin(), fg);
                Value::Real(v)
            }
            GtqScrollbarV => {
                let (x, y, h, val, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]));
                let w = 10.0;
                let (hover, down, myf) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click(), my)
                };
                let mut v = val;
                if hover && down && maxv > 0.0 && h > 0.0 {
                    v = ((myf - y) / h).clamp(0.0, 1.0) * maxv;
                }
                let (track, thumb) = (parse_color("#1e293b")?, parse_color("#475569")?);
                let t = if maxv > 0.0 { (v / maxv).clamp(0.0, 1.0) } else { 0.0 };
                let thumb_h = (h * 0.2).max(20.0);
                let ty = y + t * (h - thumb_h);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, track);
                g.canvas.rect(x, ty, w, thumb_h, thumb);
                Value::Real(v)
            }
            GtqDropdown => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let opts: Vec<String> = text(&args[3]).split(',').map(|s| s.trim().to_string()).collect();
                let n = opts.len().max(1);
                let idx = int(&args[4]).rem_euclid(n as i64) as usize;
                let h = 28.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let new_idx = if hover && down && !self.prev_down { (idx + 1) % n } else { idx };
                let (bg, white) = (parse_color(if hover { "#334155" } else { "#1e293b" })?, parse_color("#e2e8f0")?);
                let shown = opts.get(new_idx).cloned().unwrap_or_default();
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 6.0, &shown, white)?;
                draw_icon(&mut g.canvas, "chevron_down", x + w - 22.0, y + 6.0, 16.0, white);
                Value::Int(new_idx as i64)
            }
            GtqCheckboxCard => {
                let (x, y, w) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let label = text(&args[3]).to_string();
                let checked = matches!(&args[4], Value::Bool(true));
                let h = 36.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let new = if hover && down && !self.prev_down { !checked } else { checked };
                let bg = parse_color(if new {
                    "#1e3a5f"
                } else if hover {
                    "#1e293b"
                } else {
                    "#172033"
                })?;
                let (border, white, mark) =
                    (parse_color(if new { "#3b82f6" } else { "#334155" })?, parse_color("#e2e8f0")?, parse_color("#22c55e")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, w, h, bg);
                g.canvas.line(x, y, x + w, y, border);
                g.canvas.line(x, y + h, x + w, y + h, border);
                g.canvas.line(x, y, x, y + h, border);
                g.canvas.line(x + w, y, x + w, y + h, border);
                g.canvas.rect(x + 10.0, y + 11.0, 14.0, 14.0, border);
                if new {
                    draw_icon(&mut g.canvas, "check", x + 10.0, y + 11.0, 14.0, mark);
                }
                self.fonts.draw(&mut g.canvas, x + 34.0, y + 11.0, &label, white)?;
                Value::Bool(new)
            }
            GtqToggleIcon => {
                let (x, y, size) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let icon = text(&args[3]).to_string();
                let on = matches!(&args[4], Value::Bool(true));
                let bx = size + 12.0;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + bx && my >= y && my <= y + bx, g.click())
                };
                let new = if hover && down && !self.prev_down { !on } else { on };
                let bg = parse_color(if new {
                    "#3b82f6"
                } else if hover {
                    "#334155"
                } else {
                    "#1e293b"
                })?;
                let fg = parse_color(if new { "#ffffff" } else { "#94a3b8" })?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, bx, bx, bg);
                draw_icon(&mut g.canvas, &icon, x + 6.0, y + 6.0, size, fg);
                Value::Bool(new)
            }
            GtqTextButton => {
                let (x, y) = (real(&args[0]), real(&args[1]));
                let label = text(&args[2]).to_string();
                let base = parse_color(text(&args[3]))?;
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + tw && my >= y && my <= y + 18.0, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                self.fonts.draw(&mut g.canvas, x, y, &label, base)?;
                if hover {
                    g.canvas.line(x, y + 16.0, x + tw, y + 16.0, base);
                }
                Value::Bool(clicked)
            }
            GtqSliderMini => {
                let (x, y, w, val) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let (hover, down, mxf) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (my >= y - 6.0 && my <= y + 6.0 && mx >= x && mx <= x + w, g.click(), mx)
                };
                let mut v = val;
                if hover && down && w > 0.0 {
                    v = ((mxf - x) / w).clamp(0.0, 1.0);
                }
                let (track, knob) = (parse_color("#374151")?, parse_color("#60a5fa")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y - 2.0, w, 4.0, track);
                g.canvas.circle(x + v * w, y, 6.0, knob);
                Value::Real(v)
            }
            GtqStepperF => {
                let (x, y, val, step, minv, maxv) =
                    (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]), real(&args[4]), real(&args[5]));
                let (bw, fw, h) = (22.0, 60.0, 24.0);
                let (mx, my, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (a, b) = g.mouse();
                    (a as f64, b as f64, g.click())
                };
                let edge = down && !self.prev_down;
                let over = |bx: f64| mx >= bx && mx <= bx + bw && my >= y && my <= y + h;
                let mut v = val;
                if edge && over(x) {
                    v = (v - step).max(minv);
                }
                if edge && over(x + bw + fw) {
                    v = (v + step).min(maxv);
                }
                let (btn, box_c, white) =
                    (parse_color("#4a5568")?, parse_color("#1f2937")?, parse_color("#e2e8f0")?);
                let txt = crate::vm::format_real(v);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, bw, h, btn);
                g.canvas.rect(x + bw, y, fw, h, box_c);
                g.canvas.rect(x + bw + fw, y, bw, h, btn);
                self.fonts.draw(&mut g.canvas, x + 8.0, y + 4.0, "-", white)?;
                self.fonts.draw(&mut g.canvas, x + bw + fw + 7.0, y + 4.0, "+", white)?;
                self.fonts.draw(&mut g.canvas, x + bw + 8.0, y + 4.0, &txt, white)?;
                Value::Real(v)
            }
            GtqCloseButton => {
                let (x, y, size) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + size && my >= y && my <= y + size, g.click())
                };
                let clicked = hover && down && !self.prev_down;
                let (bg, fg) = (parse_color(if hover { "#ef4444" } else { "#334155" })?, parse_color("#ffffff")?);
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.rect(x, y, size, size, bg);
                draw_icon(&mut g.canvas, "close", x + size * 0.2, y + size * 0.2, size * 0.6, fg);
                Value::Bool(clicked)
            }

            GtqIcon => {
                let (x, y, sz) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let name = text(&args[3]).to_string();
                let col = parse_color(text(&args[4]))?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                draw_icon(&mut g.canvas, &name, x, y, sz, col);
                Value::Null
            }
            GtqIcCheck | GtqIcClose | GtqIcPlus | GtqIcMinus | GtqIcUp | GtqIcDown | GtqIcLeft
            | GtqIcRight | GtqIcChevronUp | GtqIcChevronDown | GtqIcChevronLeft | GtqIcChevronRight
            | GtqIcMenu | GtqIcKebab | GtqIcGear | GtqIcHome | GtqIcSearch | GtqIcHeart | GtqIcStar
            | GtqIcUser | GtqIcBell | GtqIcMail | GtqIcTrash | GtqIcPencil | GtqIcSave | GtqIcFolder
            | GtqIcFile | GtqIcPlay | GtqIcPause | GtqIcStop | GtqIcNext | GtqIcPrev | GtqIcVolume
            | GtqIcMute | GtqIcWifi | GtqIcBattery | GtqIcLock | GtqIcUnlock | GtqIcEye | GtqIcEyeOff
            | GtqIcCalendar | GtqIcClock | GtqIcPin | GtqIcTag | GtqIcFlag | GtqIcBookmark
            | GtqIcDownload | GtqIcUpload | GtqIcRefresh => {
                let name = match b {
                    GtqIcCheck => "check",
                    GtqIcClose => "close",
                    GtqIcPlus => "plus",
                    GtqIcMinus => "minus",
                    GtqIcUp => "up",
                    GtqIcDown => "down",
                    GtqIcLeft => "left",
                    GtqIcRight => "right",
                    GtqIcChevronUp => "chevron_up",
                    GtqIcChevronDown => "chevron_down",
                    GtqIcChevronLeft => "chevron_left",
                    GtqIcChevronRight => "chevron_right",
                    GtqIcMenu => "menu",
                    GtqIcKebab => "kebab",
                    GtqIcGear => "gear",
                    GtqIcHome => "home",
                    GtqIcSearch => "search",
                    GtqIcHeart => "heart",
                    GtqIcStar => "star",
                    GtqIcUser => "user",
                    GtqIcBell => "bell",
                    GtqIcMail => "mail",
                    GtqIcTrash => "trash",
                    GtqIcPencil => "pencil",
                    GtqIcSave => "save",
                    GtqIcFolder => "folder",
                    GtqIcFile => "file",
                    GtqIcPlay => "play",
                    GtqIcPause => "pause",
                    GtqIcStop => "stop",
                    GtqIcNext => "next",
                    GtqIcPrev => "prev",
                    GtqIcVolume => "volume",
                    GtqIcMute => "mute",
                    GtqIcWifi => "wifi",
                    GtqIcBattery => "battery",
                    GtqIcLock => "lock",
                    GtqIcUnlock => "unlock",
                    GtqIcEye => "eye",
                    GtqIcEyeOff => "eye_off",
                    GtqIcCalendar => "calendar",
                    GtqIcClock => "clock",
                    GtqIcPin => "pin",
                    GtqIcTag => "tag",
                    GtqIcFlag => "flag",
                    GtqIcBookmark => "bookmark",
                    GtqIcDownload => "download",
                    GtqIcUpload => "upload",
                    _ => "refresh",
                };
                let (x, y, sz) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let col = parse_color(text(&args[3]))?;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                draw_icon(&mut g.canvas, name, x, y, sz, col);
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
