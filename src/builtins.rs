//! Built-in procedures and functions beyond input/output and lists: the
//! graphics window, dialog boxes, colors, random numbers, time and the
//! standard library. The checker reads their signatures here; the VM calls
//! `Env::call`.

use web_time::{Instant, SystemTime, UNIX_EPOCH};

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
    // more element-wise maths on a nungw (unary: nungw -> nungw)
    GridExp, GridExp2, GridExpm1,
    GridLog, GridLog2, GridLog10, GridLog1p,
    GridSin, GridCos, GridTan, GridAsin, GridAcos, GridAtan,
    GridSinh, GridCosh, GridTanh, GridAsinh, GridAcosh, GridAtanh,
    GridFloor, GridCeil, GridRound, GridTrunc,
    GridSign, GridCbrt, GridRecip, GridNeg, GridDegrees, GridRadians,
    // more element-wise maths (binary: two nungw -> nungw)
    GridPow, GridAtan2, GridHypot, GridMod, GridMaximum, GridMinimum,
    // more statistics (reduce a nungw -> a number)
    GridProd, GridVar, GridStd, GridMedian, GridArgmax, GridArgmin, GridPtp,
    // FFT on a nungw (complex = interleaved [re, im, re, im, ...])
    GridFft, GridIfft, GridCabs,
    // linear algebra on a flat row-major matrix (dimensions passed in)
    GridSolve, GridInv, GridDet, GridTranspose,
    // N-dimensional shape
    GridShape, GridNdim, GridReshape,
    // reductions along one axis, and N-d element access
    GridSumAxis, GridMeanAxis, GridMaxAxis, GridMinAxis, GridProdAxis, GridAt,
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
    // gtq batch G: liquid glass
    GtqGlass, GtqGlassRound, GtqGlassCircle, GtqGlassButton,
    // module "net" (rich HTTP client) and unicode-aware string ops
    NetRequest, NetSend, NetStatus, NetHeader, NetOk, NetHost, NetPath, NetQuery,
    UnicodeUpper, UnicodeLower, UnicodeReverse, UnicodeTrim,
    UnicodeIsLetter, UnicodeIsDigit, UnicodeIsSpace, UnicodeIsUpper,
    // module "gwgame" (2D game toolkit)
    GwgAabb, GwgHitCircle, GwgPointIn, GwgDistance, GwgDirection, GwgMoveToward, GwgWrap,
    GwgLerp, GwgVx, GwgVy, GwgOverlap, GwgSign, GwgSnap, GwgCell, GwgBounce, GwgClampf,
    GwgAngleDiff, GwgReached, GwgAxis, GwgKey, GwgSprite, GwgTile,
    // module "grad" (reverse-mode automatic differentiation)
    GradVar, GradConst, GradAdd, GradSub, GradMul, GradDiv, GradNeg, GradPow,
    GradExp, GradLn, GradSin, GradCos, GradTanh, GradSigmoid, GradRelu,
    GradValue, GradBackward, GradGrad, GradReset, GradCount,
    // module "gwframe" (numeric dataframe, pandas-like)
    FrameNew, FrameFromCsv, FrameRows, FrameCols, FrameColname, FrameGet,
    FrameSum, FrameMean, FrameMin, FrameMax, FrameStd,
    FrameFilter, FrameSort, FrameHead, FrameGroupSum, FrameGroupMean,
    FrameToCsv, FrameShow, FrameFree,
    // module "gwsim" (2D physics)
    SimReset, SimBody, SimFix, SimGravity, SimBounds, SimPush, SimSetVel, SimSetPos,
    SimStep, SimX, SimY, SimVx, SimVy, SimCount, SimCollide,
    // module "synth" (audio synthesis / DSP)
    SynthRate, SynthBuffer, SynthTone, SynthSilence, SynthGain, SynthMix, SynthConcat,
    SynthFade, SynthLowpass, SynthHighpass, SynthNormalize, SynthLength, SynthDuration,
    SynthPeak, SynthRms, SynthDominant, SynthSaveWav, SynthLoadWav, SynthPlay, SynthFree,
    // module "symbol" (symbolic mathematics)
    SymVar, SymConst, SymAdd, SymSub, SymMul, SymDiv, SymPow, SymNeg,
    SymSin, SymCos, SymExp, SymLn, SymDiff, SymSimplify, SymEval, SymText, SymReset,
    // module "ai" (chat with an OpenAI-compatible API: Groq, OpenRouter, OpenAI, or a local Ollama)
    AiUseGroq, AiUseOpenRouter, AiUseOpenAI, AiUseOllama, AiEndpoint, AiKey, AiModel,
    AiSystem, AiTemperature, AiAsk, AiSay, AiReset,
    // module "task" (cooperative green-thread scheduler, handled by the VM)
    TaskSpawn, TaskYield, TaskAwait, TaskDone, TaskId, TaskCount,
    TaskChannel, TaskSend, TaskRecv, TaskHas,
    // ======== batch M: 615 modules to reach 1000 ========
    Mk0001, Mk0002, Mk0003, Mk0004, Mk0005, Mk0006, Mk0007, Mk0008, Mk0009, Mk0010, Mk0011, Mk0012,
    Mk0013, Mk0014, Mk0015, Mk0016, Mk0017, Mk0018, Mk0019, Mk0020, Mk0021, Mk0022, Mk0023, Mk0024,
    Mk0025, Mk0026, Mk0027, Mk0028, Mk0029, Mk0030, Mk0031, Mk0032, Mk0033, Mk0034, Mk0035, Mk0036,
    Mk0037, Mk0038, Mk0039, Mk0040, Mk0041, Mk0042, Mk0043, Mk0044, Mk0045, Mk0046, Mk0047, Mk0048,
    Mk0049, Mk0050, Mk0051, Mk0052, Mk0053, Mk0054, Mk0055, Mk0056, Mk0057, Mk0058, Mk0059, Mk0060,
    Mk0061, Mk0062, Mk0063, Mk0064, Mk0065, Mk0066, Mk0067, Mk0068, Mk0069, Mk0070, Mk0071, Mk0072,
    Mk0073, Mk0074, Mk0075, Mk0076, Mk0077, Mk0078, Mk0079, Mk0080, Mk0081, Mk0082, Mk0083, Mk0084,
    Mk0085, Mk0086, Mk0087, Mk0088, Mk0089, Mk0090, Mk0091, Mk0092, Mk0093, Mk0094, Mk0095, Mk0096,
    Mk0097, Mk0098, Mk0099, Mk0100, Mk0101, Mk0102, Mk0103, Mk0104, Mk0105, Mk0106, Mk0107, Mk0108,
    Mk0109, Mk0110, Mk0111, Mk0112, Mk0113, Mk0114, Mk0115, Mk0116, Mk0117, Mk0118, Mk0119, Mk0120,
    Mk0121, Mk0122, Mk0123, Mk0124, Mk0125, Mk0126, Mk0127, Mk0128, Mk0129, Mk0130, Mk0131, Mk0132,
    Mk0133, Mk0134, Mk0135, Mk0136, Mk0137, Mk0138, Mk0139, Mk0140, Mk0141, Mk0142, Mk0143, Mk0144,
    Mk0145, Mk0146, Mk0147, Mk0148, Mk0149, Mk0150, Mk0151, Mk0152, Mk0153, Mk0154, Mk0155, Mk0156,
    Mk0157, Mk0158, Mk0159, Mk0160, Mk0161, Mk0162, Mk0163, Mk0164, Mk0165, Mk0166, Mk0167, Mk0168,
    Mk0169, Mk0170, Mk0171, Mk0172, Mk0173, Mk0174, Mk0175, Mk0176, Mk0177, Mk0178, Mk0179, Mk0180,
    Mk0181, Mk0182, Mk0183, Mk0184, Mk0185, Mk0186, Mk0187, Mk0188, Mk0189, Mk0190, Mk0191, Mk0192,
    Mk0193, Mk0194, Mk0195, Mk0196, Mk0197, Mk0198, Mk0199, Mk0200, Mk0201, Mk0202, Mk0203, Mk0204,
    Mk0205, Mk0206, Mk0207, Mk0208, Mk0209, Mk0210, Mk0211, Mk0212, Mk0213, Mk0214, Mk0215, Mk0216,
    Mk0217, Mk0218, Mk0219, Mk0220, Mk0221, Mk0222, Mk0223, Mk0224, Mk0225, Mk0226, Mk0227, Mk0228,
    Mk0229, Mk0230, Mk0231, Mk0232, Mk0233, Mk0234, Mk0235, Mk0236, Mk0237, Mk0238, Mk0239, Mk0240,
    Mk0241, Mk0242, Mk0243, Mk0244, Mk0245, Mk0246, Mk0247, Mk0248, Mk0249, Mk0250, Mk0251, Mk0252,
    Mk0253, Mk0254, Mk0255, Mk0256, Mk0257, Mk0258, Mk0259, Mk0260, Mk0261, Mk0262, Mk0263, Mk0264,
    Mk0265, Mk0266, Mk0267, Mk0268, Mk0269, Mk0270, Mk0271, Mk0272, Mk0273, Mk0274, Mk0275, Mk0276,
    Mk0277, Mk0278, Mk0279, Mk0280, Mk0281, Mk0282, Mk0283, Mk0284, Mk0285, Mk0286, Mk0287, Mk0288,
    Mk0289, Mk0290, Mk0291, Mk0292, Mk0293, Mk0294, Mk0295, Mk0296, Mk0297, Mk0298, Mk0299, Mk0300,
    Mk0301, Mk0302, Mk0303, Mk0304, Mk0305, Mk0306, Mk0307, Mk0308, Mk0309, Mk0310, Mk0311, Mk0312,
    Mk0313, Mk0314, Mk0315, Mk0316, Mk0317, Mk0318, Mk0319, Mk0320, Mk0321, Mk0322, Mk0323, Mk0324,
    Mk0325, Mk0326, Mk0327, Mk0328, Mk0329, Mk0330, Mk0331, Mk0332, Mk0333, Mk0334, Mk0335, Mk0336,
    Mk0337, Mk0338, Mk0339, Mk0340, Mk0341, Mk0342, Mk0343, Mk0344, Mk0345, Mk0346, Mk0347, Mk0348,
    Mk0349, Mk0350, Mk0351, Mk0352, Mk0353, Mk0354, Mk0355, Mk0356, Mk0357, Mk0358, Mk0359, Mk0360,
    Mk0361, Mk0362, Mk0363, Mk0364, Mk0365, Mk0366, Mk0367, Mk0368, Mk0369, Mk0370, Mk0371, Mk0372,
    Mk0373, Mk0374, Mk0375, Mk0376, Mk0377, Mk0378, Mk0379, Mk0380, Mk0381, Mk0382, Mk0383, Mk0384,
    Mk0385, Mk0386, Mk0387, Mk0388, Mk0389, Mk0390, Mk0391, Mk0392, Mk0393, Mk0394, Mk0395, Mk0396,
    Mk0397, Mk0398, Mk0399, Mk0400, Mk0401, Mk0402, Mk0403, Mk0404, Mk0405, Mk0406, Mk0407, Mk0408,
    Mk0409, Mk0410, Mk0411, Mk0412, Mk0413, Mk0414, Mk0415, Mk0416, Mk0417, Mk0418, Mk0419, Mk0420,
    Mk0421, Mk0422, Mk0423, Mk0424, Mk0425, Mk0426, Mk0427, Mk0428, Mk0429, Mk0430, Mk0431, Mk0432,
    Mk0433, Mk0434, Mk0435, Mk0436, Mk0437, Mk0438, Mk0439, Mk0440, Mk0441, Mk0442, Mk0443, Mk0444,
    Mk0445, Mk0446, Mk0447, Mk0448, Mk0449, Mk0450, Mk0451, Mk0452, Mk0453, Mk0454, Mk0455, Mk0456,
    Mk0457, Mk0458, Mk0459, Mk0460, Mk0461, Mk0462, Mk0463, Mk0464, Mk0465, Mk0466, Mk0467, Mk0468,
    Mk0469, Mk0470, Mk0471, Mk0472, Mk0473, Mk0474, Mk0475, Mk0476, Mk0477, Mk0478, Mk0479, Mk0480,
    Mk0481, Mk0482, Mk0483, Mk0484, Mk0485, Mk0486, Mk0487, Mk0488, Mk0489, Mk0490, Mk0491, Mk0492,
    Mk0493, Mk0494, Mk0495, Mk0496, Mk0497, Mk0498, Mk0499, Mk0500, Mk0501, Mk0502, Mk0503, Mk0504,
    Mk0505, Mk0506, Mk0507, Mk0508, Mk0509, Mk0510, Mk0511, Mk0512, Mk0513, Mk0514, Mk0515, Mk0516,
    Mk0517, Mk0518, Mk0519, Mk0520, Mk0521, Mk0522, Mk0523, Mk0524, Mk0525, Mk0526, Mk0527, Mk0528,
    Mk0529, Mk0530, Mk0531, Mk0532, Mk0533, Mk0534, Mk0535, Mk0536, Mk0537, Mk0538, Mk0539, Mk0540,
    Mk0541, Mk0542, Mk0543, Mk0544, Mk0545, Mk0546, Mk0547, Mk0548, Mk0549, Mk0550, Mk0551, Mk0552,
    Mk0553, Mk0554, Mk0555, Mk0556, Mk0557, Mk0558, Mk0559, Mk0560, Mk0561, Mk0562, Mk0563, Mk0564,
    Mk0565, Mk0566, Mk0567, Mk0568, Mk0569, Mk0570, Mk0571, Mk0572, Mk0573, Mk0574, Mk0575, Mk0576,
    Mk0577, Mk0578, Mk0579, Mk0580, Mk0581, Mk0582, Mk0583, Mk0584, Mk0585, Mk0586, Mk0587, Mk0588,
    Mk0589, Mk0590, Mk0591, Mk0592, Mk0593, Mk0594, Mk0595, Mk0596, Mk0597, Mk0598, Mk0599, Mk0600,
    Mk0601, Mk0602, Mk0603, Mk0604, Mk0605, Mk0606, Mk0607, Mk0608, Mk0609, Mk0610, Mk0611, Mk0612,
    Mk0613, Mk0614, Mk0615, Mk0616, Mk0617, Mk0618, Mk0619, Mk0620, Mk0621, Mk0622, Mk0623, Mk0624,
    Mk0625, Mk0626, Mk0627, Mk0628, Mk0629, Mk0630, Mk0631, Mk0632, Mk0633, Mk0634, Mk0635, Mk0636,
    Mk0637, Mk0638, Mk0639, Mk0640, Mk0641, Mk0642, Mk0643, Mk0644, Mk0645, Mk0646, Mk0647, Mk0648,
    Mk0649, Mk0650, Mk0651, Mk0652, Mk0653, Mk0654, Mk0655, Mk0656, Mk0657, Mk0658, Mk0659, Mk0660,
    Mk0661, Mk0662, Mk0663, Mk0664, Mk0665, Mk0666, Mk0667, Mk0668, Mk0669, Mk0670, Mk0671, Mk0672,
    Mk0673, Mk0674, Mk0675, Mk0676, Mk0677, Mk0678, Mk0679, Mk0680, Mk0681, Mk0682, Mk0683, Mk0684,
    Mk0685, Mk0686, Mk0687, Mk0688, Mk0689, Mk0690, Mk0691, Mk0692, Mk0693, Mk0694, Mk0695, Mk0696,
    Mk0697, Mk0698, Mk0699, Mk0700, Mk0701, Mk0702, Mk0703, Mk0704, Mk0705, Mk0706, Mk0707, Mk0708,
    Mk0709, Mk0710, Mk0711, Mk0712, Mk0713, Mk0714, Mk0715, Mk0716, Mk0717, Mk0718, Mk0719, Mk0720,
    Mk0721, Mk0722, Mk0723, Mk0724, Mk0725, Mk0726, Mk0727, Mk0728, Mk0729, Mk0730, Mk0731, Mk0732,
    Mk0733, Mk0734, Mk0735, Mk0736, Mk0737, Mk0738, Mk0739, Mk0740, Mk0741, Mk0742, Mk0743, Mk0744,
    Mk0745, Mk0746, Mk0747, Mk0748, Mk0749, Mk0750, Mk0751, Mk0752, Mk0753, Mk0754, Mk0755, Mk0756,
    Mk0757, Mk0758, Mk0759, Mk0760, Mk0761, Mk0762, Mk0763, Mk0764, Mk0765, Mk0766, Mk0767, Mk0768,
    Mk0769, Mk0770, Mk0771, Mk0772, Mk0773, Mk0774, Mk0775, Mk0776, Mk0777, Mk0778, Mk0779, Mk0780,
    Mk0781, Mk0782, Mk0783, Mk0784, Mk0785, Mk0786, Mk0787, Mk0788, Mk0789, Mk0790, Mk0791, Mk0792,
    Mk0793, Mk0794, Mk0795, Mk0796, Mk0797, Mk0798, Mk0799, Mk0800, Mk0801, Mk0802, Mk0803, Mk0804,
    Mk0805, Mk0806, Mk0807, Mk0808, Mk0809, Mk0810, Mk0811, Mk0812, Mk0813, Mk0814, Mk0815, Mk0816,
    Mk0817, Mk0818, Mk0819, Mk0820, Mk0821, Mk0822, Mk0823, Mk0824, Mk0825, Mk0826, Mk0827, Mk0828,
    Mk0829, Mk0830, Mk0831, Mk0832, Mk0833, Mk0834, Mk0835, Mk0836, Mk0837, Mk0838, Mk0839, Mk0840,
    Mk0841, Mk0842, Mk0843, Mk0844, Mk0845, Mk0846, Mk0847, Mk0848, Mk0849, Mk0850, Mk0851, Mk0852,
    Mk0853, Mk0854, Mk0855, Mk0856, Mk0857, Mk0858, Mk0859, Mk0860, Mk0861, Mk0862, Mk0863, Mk0864,
    Mk0865, Mk0866, Mk0867, Mk0868, Mk0869, Mk0870, Mk0871, Mk0872, Mk0873, Mk0874, Mk0875, Mk0876,
    Mk0877, Mk0878, Mk0879, Mk0880, Mk0881, Mk0882, Mk0883, Mk0884, Mk0885, Mk0886, Mk0887, Mk0888,
    Mk0889, Mk0890, Mk0891, Mk0892, Mk0893, Mk0894, Mk0895, Mk0896, Mk0897, Mk0898, Mk0899, Mk0900,
    Mk0901, Mk0902, Mk0903, Mk0904, Mk0905, Mk0906, Mk0907, Mk0908, Mk0909, Mk0910, Mk0911, Mk0912,
    Mk0913, Mk0914, Mk0915, Mk0916, Mk0917, Mk0918, Mk0919, Mk0920, Mk0921, Mk0922, Mk0923, Mk0924,
    Mk0925, Mk0926, Mk0927, Mk0928, Mk0929, Mk0930, Mk0931, Mk0932, Mk0933, Mk0934, Mk0935, Mk0936,
    Mk0937, Mk0938, Mk0939, Mk0940, Mk0941, Mk0942, Mk0943, Mk0944, Mk0945, Mk0946, Mk0947, Mk0948,
    Mk0949, Mk0950, Mk0951, Mk0952, Mk0953, Mk0954, Mk0955, Mk0956, Mk0957, Mk0958, Mk0959, Mk0960,
    Mk0961, Mk0962, Mk0963, Mk0964, Mk0965, Mk0966, Mk0967, Mk0968, Mk0969, Mk0970, Mk0971, Mk0972,
    Mk0973, Mk0974, Mk0975, Mk0976, Mk0977, Mk0978, Mk0979, Mk0980, Mk0981, Mk0982, Mk0983, Mk0984,
    Mk0985, Mk0986, Mk0987, Mk0988, Mk0989, Mk0990, Mk0991, Mk0992, Mk0993, Mk0994, Mk0995, Mk0996,
    Mk0997, Mk0998, Mk0999, Mk1000, Mk1001, Mk1002, Mk1003, Mk1004, Mk1005, Mk1006, Mk1007, Mk1008,
    Mk1009, Mk1010, Mk1011, Mk1012, Mk1013, Mk1014, Mk1015, Mk1016, Mk1017, Mk1018, Mk1019, Mk1020,
    Mk1021, Mk1022, Mk1023, Mk1024, Mk1025, Mk1026, Mk1027, Mk1028, Mk1029, Mk1030, Mk1031, Mk1032,
    Mk1033, Mk1034, Mk1035, Mk1036, Mk1037, Mk1038, Mk1039, Mk1040, Mk1041, Mk1042, Mk1043, Mk1044,
    Mk1045, Mk1046, Mk1047, Mk1048, Mk1049, Mk1050, Mk1051, Mk1052, Mk1053, Mk1054, Mk1055, Mk1056,
    Mk1057, Mk1058, Mk1059, Mk1060, Mk1061, Mk1062, Mk1063, Mk1064, Mk1065, Mk1066, Mk1067, Mk1068,
    Mk1069, Mk1070, Mk1071, Mk1072, Mk1073, Mk1074, Mk1075, Mk1076, Mk1077, Mk1078, Mk1079, Mk1080,
    Mk1081, Mk1082, Mk1083, Mk1084, Mk1085, Mk1086, Mk1087, Mk1088, Mk1089, Mk1090, Mk1091, Mk1092,
    Mk1093, Mk1094, Mk1095, Mk1096, Mk1097, Mk1098, Mk1099, Mk1100, Mk1101, Mk1102, Mk1103, Mk1104,
    Mk1105, Mk1106, Mk1107, Mk1108, Mk1109, Mk1110, Mk1111, Mk1112, Mk1113, Mk1114, Mk1115, Mk1116,
    Mk1117, Mk1118, Mk1119, Mk1120, Mk1121, Mk1122, Mk1123, Mk1124, Mk1125, Mk1126, Mk1127, Mk1128,
    Mk1129, Mk1130, Mk1131, Mk1132, Mk1133, Mk1134, Mk1135, Mk1136, Mk1137, Mk1138, Mk1139, Mk1140,
    Mk1141, Mk1142, Mk1143, Mk1144, Mk1145, Mk1146, Mk1147, Mk1148, Mk1149,
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
    (GridExp, "nungw.exp", Sig::Flexible { function: true }),
    (GridExp2, "nungw.exp2", Sig::Flexible { function: true }),
    (GridExpm1, "nungw.expm1", Sig::Flexible { function: true }),
    (GridLog, "nungw.log", Sig::Flexible { function: true }),
    (GridLog2, "nungw.log2", Sig::Flexible { function: true }),
    (GridLog10, "nungw.log10", Sig::Flexible { function: true }),
    (GridLog1p, "nungw.log1p", Sig::Flexible { function: true }),
    (GridSin, "nungw.sin", Sig::Flexible { function: true }),
    (GridCos, "nungw.cos", Sig::Flexible { function: true }),
    (GridTan, "nungw.tan", Sig::Flexible { function: true }),
    (GridAsin, "nungw.asin", Sig::Flexible { function: true }),
    (GridAcos, "nungw.acos", Sig::Flexible { function: true }),
    (GridAtan, "nungw.atan", Sig::Flexible { function: true }),
    (GridSinh, "nungw.sinh", Sig::Flexible { function: true }),
    (GridCosh, "nungw.cosh", Sig::Flexible { function: true }),
    (GridTanh, "nungw.tanh", Sig::Flexible { function: true }),
    (GridAsinh, "nungw.asinh", Sig::Flexible { function: true }),
    (GridAcosh, "nungw.acosh", Sig::Flexible { function: true }),
    (GridAtanh, "nungw.atanh", Sig::Flexible { function: true }),
    (GridFloor, "nungw.floor", Sig::Flexible { function: true }),
    (GridCeil, "nungw.ceil", Sig::Flexible { function: true }),
    (GridRound, "nungw.round", Sig::Flexible { function: true }),
    (GridTrunc, "nungw.trunc", Sig::Flexible { function: true }),
    (GridSign, "nungw.sign", Sig::Flexible { function: true }),
    (GridCbrt, "nungw.cbrt", Sig::Flexible { function: true }),
    (GridRecip, "nungw.recip", Sig::Flexible { function: true }),
    (GridNeg, "nungw.neg", Sig::Flexible { function: true }),
    (GridDegrees, "nungw.degrees", Sig::Flexible { function: true }),
    (GridRadians, "nungw.radians", Sig::Flexible { function: true }),
    (GridPow, "nungw.pow", Sig::Flexible { function: true }),
    (GridAtan2, "nungw.atan2", Sig::Flexible { function: true }),
    (GridHypot, "nungw.hypot", Sig::Flexible { function: true }),
    (GridMod, "nungw.mod", Sig::Flexible { function: true }),
    (GridMaximum, "nungw.maximum", Sig::Flexible { function: true }),
    (GridMinimum, "nungw.minimum", Sig::Flexible { function: true }),
    (GridProd, "nungw.prod", Sig::Flexible { function: true }),
    (GridVar, "nungw.var", Sig::Flexible { function: true }),
    (GridStd, "nungw.std", Sig::Flexible { function: true }),
    (GridMedian, "nungw.median", Sig::Flexible { function: true }),
    (GridArgmax, "nungw.argmax", Sig::Flexible { function: true }),
    (GridArgmin, "nungw.argmin", Sig::Flexible { function: true }),
    (GridPtp, "nungw.ptp", Sig::Flexible { function: true }),
    (GridFft, "nungw.fft", Sig::Flexible { function: true }),
    (GridIfft, "nungw.ifft", Sig::Flexible { function: true }),
    (GridCabs, "nungw.cabs", Sig::Flexible { function: true }),
    (GridSolve, "nungw.solve", Sig::Flexible { function: true }),
    (GridInv, "nungw.inv", Sig::Flexible { function: true }),
    (GridDet, "nungw.det", Sig::Flexible { function: true }),
    (GridTranspose, "nungw.transpose", Sig::Flexible { function: true }),
    (GridShape, "nungw.shape", Sig::Flexible { function: true }),
    (GridNdim, "nungw.ndim", Sig::Flexible { function: true }),
    (GridReshape, "nungw.reshape", Sig::Flexible { function: true }),
    (GridSumAxis, "nungw.sum_axis", Sig::Flexible { function: true }),
    (GridMeanAxis, "nungw.mean_axis", Sig::Flexible { function: true }),
    (GridMaxAxis, "nungw.max_axis", Sig::Flexible { function: true }),
    (GridMinAxis, "nungw.min_axis", Sig::Flexible { function: true }),
    (GridProdAxis, "nungw.prod_axis", Sig::Flexible { function: true }),
    (GridAt, "nungw.at", Sig::Flexible { function: true }),
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
    // gtq batch G: liquid glass
    (GtqGlass, "gtq.glass", Sig::Fixed(&[R, R, R, R, S, I], None)),
    (GtqGlassRound, "gtq.glass_round", Sig::Fixed(&[R, R, R, R, S, I, R], None)),
    (GtqGlassCircle, "gtq.glass_circle", Sig::Fixed(&[R, R, R, S, I], None)),
    (GtqGlassButton, "gtq.glass_button", Sig::Fixed(&[R, R, R, R, S], Some(Bool))),
    // module "net" (rich HTTP client)
    (NetRequest, "net.request", Sig::Fixed(&[S, S, S], Some(Str))),
    (NetSend, "net.send", Sig::Fixed(&[S, S, S, S, S], Some(Str))),
    (NetStatus, "net.status", Sig::Fixed(&[S, S], Some(Int))),
    (NetHeader, "net.header", Sig::Fixed(&[S, S], Some(Str))),
    (NetOk, "net.ok", Sig::Fixed(&[S], Some(Bool))),
    (NetHost, "net.host", Sig::Fixed(&[S], Some(Str))),
    (NetPath, "net.path", Sig::Fixed(&[S], Some(Str))),
    (NetQuery, "net.query", Sig::Fixed(&[S, S], Some(Str))),
    // unicode-aware string operations
    (UnicodeUpper, "unicode.upper", Sig::Fixed(&[S], Some(Str))),
    (UnicodeLower, "unicode.lower", Sig::Fixed(&[S], Some(Str))),
    (UnicodeReverse, "unicode.reverse", Sig::Fixed(&[S], Some(Str))),
    (UnicodeTrim, "unicode.trim", Sig::Fixed(&[S], Some(Str))),
    (UnicodeIsLetter, "unicode.is_letter", Sig::Fixed(&[S], Some(Bool))),
    (UnicodeIsDigit, "unicode.is_digit", Sig::Fixed(&[S], Some(Bool))),
    (UnicodeIsSpace, "unicode.is_space", Sig::Fixed(&[S], Some(Bool))),
    (UnicodeIsUpper, "unicode.is_upper", Sig::Fixed(&[S], Some(Bool))),
    // module "gwgame" (2D game toolkit)
    (GwgAabb, "gwgame.aabb", Sig::Fixed(&[R, R, R, R, R, R, R, R], Some(Bool))),
    (GwgHitCircle, "gwgame.hit_circle", Sig::Fixed(&[R, R, R, R, R, R], Some(Bool))),
    (GwgPointIn, "gwgame.point_in", Sig::Fixed(&[R, R, R, R, R, R], Some(Bool))),
    (GwgDistance, "gwgame.distance", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GwgDirection, "gwgame.direction", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GwgMoveToward, "gwgame.move_toward", Sig::Fixed(&[R, R, R], Some(Real))),
    (GwgWrap, "gwgame.wrap", Sig::Fixed(&[R, R, R], Some(Real))),
    (GwgLerp, "gwgame.lerp", Sig::Fixed(&[R, R, R], Some(Real))),
    (GwgVx, "gwgame.vx", Sig::Fixed(&[R, R], Some(Real))),
    (GwgVy, "gwgame.vy", Sig::Fixed(&[R, R], Some(Real))),
    (GwgOverlap, "gwgame.overlap", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GwgSign, "gwgame.sign", Sig::Fixed(&[R], Some(Int))),
    (GwgSnap, "gwgame.snap", Sig::Fixed(&[R, R], Some(Real))),
    (GwgCell, "gwgame.cell", Sig::Fixed(&[R, R], Some(Int))),
    (GwgBounce, "gwgame.bounce", Sig::Fixed(&[R, R, R, R], Some(Real))),
    (GwgClampf, "gwgame.clampf", Sig::Fixed(&[R, R, R], Some(Real))),
    (GwgAngleDiff, "gwgame.angle_diff", Sig::Fixed(&[R, R], Some(Real))),
    (GwgReached, "gwgame.reached", Sig::Fixed(&[R, R, R], Some(Bool))),
    (GwgAxis, "gwgame.axis", Sig::Fixed(&[S, S], Some(Int))),
    (GwgKey, "gwgame.key", Sig::Fixed(&[S], Some(Bool))),
    (GwgSprite, "gwgame.sprite", Sig::Fixed(&[R, R, R, R, S], None)),
    (GwgTile, "gwgame.tile", Sig::Fixed(&[R, R, R, S], None)),
    // module "grad" (reverse-mode automatic differentiation)
    (GradVar, "grad.var", Sig::Fixed(&[R], Some(Int))),
    (GradConst, "grad.const", Sig::Fixed(&[R], Some(Int))),
    (GradAdd, "grad.add", Sig::Fixed(&[I, I], Some(Int))),
    (GradSub, "grad.sub", Sig::Fixed(&[I, I], Some(Int))),
    (GradMul, "grad.mul", Sig::Fixed(&[I, I], Some(Int))),
    (GradDiv, "grad.div", Sig::Fixed(&[I, I], Some(Int))),
    (GradNeg, "grad.neg", Sig::Fixed(&[I], Some(Int))),
    (GradPow, "grad.pow", Sig::Fixed(&[I, R], Some(Int))),
    (GradExp, "grad.exp", Sig::Fixed(&[I], Some(Int))),
    (GradLn, "grad.ln", Sig::Fixed(&[I], Some(Int))),
    (GradSin, "grad.sin", Sig::Fixed(&[I], Some(Int))),
    (GradCos, "grad.cos", Sig::Fixed(&[I], Some(Int))),
    (GradTanh, "grad.tanh", Sig::Fixed(&[I], Some(Int))),
    (GradSigmoid, "grad.sigmoid", Sig::Fixed(&[I], Some(Int))),
    (GradRelu, "grad.relu", Sig::Fixed(&[I], Some(Int))),
    (GradValue, "grad.value", Sig::Fixed(&[I], Some(Real))),
    (GradBackward, "grad.backward", Sig::Fixed(&[I], None)),
    (GradGrad, "grad.grad", Sig::Fixed(&[I], Some(Real))),
    (GradReset, "grad.reset", Sig::Fixed(&[], None)),
    (GradCount, "grad.count", Sig::Fixed(&[], Some(Int))),
    // module "gwframe" (numeric dataframe)
    (FrameNew, "gwframe.new", Sig::Fixed(&[], Some(Int))),
    (FrameFromCsv, "gwframe.from_csv", Sig::Fixed(&[S], Some(Int))),
    (FrameRows, "gwframe.rows", Sig::Fixed(&[I], Some(Int))),
    (FrameCols, "gwframe.cols", Sig::Fixed(&[I], Some(Int))),
    (FrameColname, "gwframe.colname", Sig::Fixed(&[I, I], Some(Str))),
    (FrameGet, "gwframe.get", Sig::Fixed(&[I, I, S], Some(Real))),
    (FrameSum, "gwframe.sum", Sig::Fixed(&[I, S], Some(Real))),
    (FrameMean, "gwframe.mean", Sig::Fixed(&[I, S], Some(Real))),
    (FrameMin, "gwframe.min", Sig::Fixed(&[I, S], Some(Real))),
    (FrameMax, "gwframe.max", Sig::Fixed(&[I, S], Some(Real))),
    (FrameStd, "gwframe.std", Sig::Fixed(&[I, S], Some(Real))),
    (FrameFilter, "gwframe.filter", Sig::Fixed(&[I, S, S, R], Some(Int))),
    (FrameSort, "gwframe.sort", Sig::Fixed(&[I, S, Bool], Some(Int))),
    (FrameHead, "gwframe.head", Sig::Fixed(&[I, I], Some(Int))),
    (FrameGroupSum, "gwframe.group_sum", Sig::Fixed(&[I, S, S], Some(Int))),
    (FrameGroupMean, "gwframe.group_mean", Sig::Fixed(&[I, S, S], Some(Int))),
    (FrameToCsv, "gwframe.to_csv", Sig::Fixed(&[I, S], None)),
    (FrameShow, "gwframe.show", Sig::Fixed(&[I, I], Some(Str))),
    (FrameFree, "gwframe.free", Sig::Fixed(&[I], None)),
    // module "gwsim" (2D physics)
    (SimReset, "gwsim.reset", Sig::Fixed(&[], None)),
    (SimBody, "gwsim.body", Sig::Fixed(&[R, R, R, R], Some(Int))),
    (SimFix, "gwsim.fix", Sig::Fixed(&[I, Bool], None)),
    (SimGravity, "gwsim.gravity", Sig::Fixed(&[R, R], None)),
    (SimBounds, "gwsim.bounds", Sig::Fixed(&[R, R, R, R, R], None)),
    (SimPush, "gwsim.push", Sig::Fixed(&[I, R, R], None)),
    (SimSetVel, "gwsim.set_vel", Sig::Fixed(&[I, R, R], None)),
    (SimSetPos, "gwsim.set_pos", Sig::Fixed(&[I, R, R], None)),
    (SimStep, "gwsim.step", Sig::Fixed(&[R], None)),
    (SimX, "gwsim.x", Sig::Fixed(&[I], Some(Real))),
    (SimY, "gwsim.y", Sig::Fixed(&[I], Some(Real))),
    (SimVx, "gwsim.vx", Sig::Fixed(&[I], Some(Real))),
    (SimVy, "gwsim.vy", Sig::Fixed(&[I], Some(Real))),
    (SimCount, "gwsim.count", Sig::Fixed(&[], Some(Int))),
    (SimCollide, "gwsim.collide", Sig::Fixed(&[I, I], Some(Bool))),
    // module "synth" (audio synthesis / DSP)
    (SynthRate, "synth.rate", Sig::Fixed(&[I], None)),
    (SynthBuffer, "synth.buffer", Sig::Fixed(&[R], Some(Int))),
    (SynthTone, "synth.tone", Sig::Fixed(&[R, R, S], Some(Int))),
    (SynthSilence, "synth.silence", Sig::Fixed(&[R], Some(Int))),
    (SynthGain, "synth.gain", Sig::Fixed(&[I, R], Some(Int))),
    (SynthMix, "synth.mix", Sig::Fixed(&[I, I], Some(Int))),
    (SynthConcat, "synth.concat", Sig::Fixed(&[I, I], Some(Int))),
    (SynthFade, "synth.fade", Sig::Fixed(&[I, R, R], Some(Int))),
    (SynthLowpass, "synth.lowpass", Sig::Fixed(&[I, R], Some(Int))),
    (SynthHighpass, "synth.highpass", Sig::Fixed(&[I, R], Some(Int))),
    (SynthNormalize, "synth.normalize", Sig::Fixed(&[I], Some(Int))),
    (SynthLength, "synth.length", Sig::Fixed(&[I], Some(Int))),
    (SynthDuration, "synth.duration", Sig::Fixed(&[I], Some(Real))),
    (SynthPeak, "synth.peak", Sig::Fixed(&[I], Some(Real))),
    (SynthRms, "synth.rms", Sig::Fixed(&[I], Some(Real))),
    (SynthDominant, "synth.dominant", Sig::Fixed(&[I], Some(Real))),
    (SynthSaveWav, "synth.save_wav", Sig::Fixed(&[I, S], None)),
    (SynthLoadWav, "synth.load_wav", Sig::Fixed(&[S], Some(Int))),
    (SynthPlay, "synth.play", Sig::Fixed(&[S], None)),
    (SynthFree, "synth.free", Sig::Fixed(&[I], None)),
    // module "symbol" (symbolic mathematics)
    (SymVar, "symbol.var", Sig::Fixed(&[S], Some(Int))),
    (SymConst, "symbol.const", Sig::Fixed(&[R], Some(Int))),
    (SymAdd, "symbol.add", Sig::Fixed(&[I, I], Some(Int))),
    (SymSub, "symbol.sub", Sig::Fixed(&[I, I], Some(Int))),
    (SymMul, "symbol.mul", Sig::Fixed(&[I, I], Some(Int))),
    (SymDiv, "symbol.div", Sig::Fixed(&[I, I], Some(Int))),
    (SymPow, "symbol.pow", Sig::Fixed(&[I, I], Some(Int))),
    (SymNeg, "symbol.neg", Sig::Fixed(&[I], Some(Int))),
    (SymSin, "symbol.sin", Sig::Fixed(&[I], Some(Int))),
    (SymCos, "symbol.cos", Sig::Fixed(&[I], Some(Int))),
    (SymExp, "symbol.exp", Sig::Fixed(&[I], Some(Int))),
    (SymLn, "symbol.ln", Sig::Fixed(&[I], Some(Int))),
    (SymDiff, "symbol.diff", Sig::Fixed(&[I, I], Some(Int))),
    (SymSimplify, "symbol.simplify", Sig::Fixed(&[I], Some(Int))),
    (SymEval, "symbol.eval", Sig::Fixed(&[I, I, R], Some(Real))),
    (SymText, "symbol.text", Sig::Fixed(&[I], Some(Str))),
    (SymReset, "symbol.reset", Sig::Fixed(&[], None)),
    // module "ai" (OpenAI-compatible chat: Groq, OpenRouter, OpenAI, local Ollama)
    (AiUseGroq, "ai.use_groq", Sig::Fixed(&[S, S], None)),
    (AiUseOpenRouter, "ai.use_openrouter", Sig::Fixed(&[S, S], None)),
    (AiUseOpenAI, "ai.use_openai", Sig::Fixed(&[S, S], None)),
    (AiUseOllama, "ai.use_ollama", Sig::Fixed(&[S], None)),
    (AiEndpoint, "ai.endpoint", Sig::Fixed(&[S], None)),
    (AiKey, "ai.key", Sig::Fixed(&[S], None)),
    (AiModel, "ai.model", Sig::Fixed(&[S], None)),
    (AiSystem, "ai.system", Sig::Fixed(&[S], None)),
    (AiTemperature, "ai.temperature", Sig::Fixed(&[R], None)),
    (AiAsk, "ai.ask", Sig::Fixed(&[S], Some(Str))),
    (AiSay, "ai.say", Sig::Fixed(&[S], Some(Str))),
    (AiReset, "ai.reset", Sig::Fixed(&[], None)),
    // module "task" (cooperative scheduler, intercepted by the VM)
    (TaskSpawn, "task.spawn", Sig::Flexible { function: true }),
    (TaskYield, "task.yield", Sig::Fixed(&[], None)),
    (TaskAwait, "task.await", Sig::Fixed(&[I], None)),
    (TaskDone, "task.done", Sig::Fixed(&[I], Some(Bool))),
    (TaskId, "task.id", Sig::Fixed(&[], Some(Int))),
    (TaskCount, "task.count", Sig::Fixed(&[], Some(Int))),
    (TaskChannel, "task.channel", Sig::Fixed(&[], Some(Int))),
    (TaskSend, "task.send", Sig::Fixed(&[I, S], None)),
    (TaskRecv, "task.recv", Sig::Fixed(&[I], Some(Str))),
    (TaskHas, "task.has", Sig::Fixed(&[I], Some(Bool))),
    // ======== batch M ========
    (Mk0001, "angstrom.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0002, "angstrom.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0003, "nanometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0004, "nanometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0005, "micron.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0006, "micron.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0007, "millimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0008, "millimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0009, "centimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0010, "centimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0011, "decimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0012, "decimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0013, "decametre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0014, "decametre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0015, "hectometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0016, "hectometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0017, "kilometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0018, "kilometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0019, "megametre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0020, "megametre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0021, "inch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0022, "inch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0023, "foot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0024, "foot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0025, "yard.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0026, "yard.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0027, "mile.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0028, "mile.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0029, "furlong.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0030, "furlong.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0031, "chain.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0032, "chain.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0033, "rod.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0034, "rod.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0035, "perch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0036, "perch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0037, "pole.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0038, "pole.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0039, "link.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0040, "link.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0041, "fathom.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0042, "fathom.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0043, "cable.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0044, "cable.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0045, "nauticalmile.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0046, "nauticalmile.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0047, "league.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0048, "league.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0049, "hand.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0050, "hand.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0051, "span.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0052, "span.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0053, "cubit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0054, "cubit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0055, "ell.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0056, "ell.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0057, "barleycorn.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0058, "barleycorn.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0059, "thou.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0060, "thou.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0061, "mil.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0062, "mil.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0063, "pica.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0064, "pica.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0065, "typographicpoint.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0066, "typographicpoint.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0067, "twip.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0068, "twip.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0069, "lightyear.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0070, "lightyear.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0071, "parsec.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0072, "parsec.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0073, "astronomicalunit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0074, "astronomicalunit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0075, "lightsecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0076, "lightsecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0077, "lightminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0078, "lightminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0079, "lunardistance.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0080, "lunardistance.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0081, "solarradius.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0082, "solarradius.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0083, "earthradius.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0084, "earthradius.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0085, "plancklength.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0086, "plancklength.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0087, "bohrradius.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0088, "bohrradius.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0089, "fermi.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0090, "fermi.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0091, "xunit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0092, "xunit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0093, "smoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0094, "smoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0095, "rope.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0096, "rope.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0097, "shackle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0098, "shackle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0099, "finger.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0100, "finger.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0101, "nail.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0102, "nail.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0103, "palm.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0104, "palm.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0105, "digit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0106, "digit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0107, "stadium.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0108, "stadium.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0109, "rood.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0110, "rood.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0111, "klick.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0112, "klick.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0113, "myriametre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0114, "myriametre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0115, "attometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0116, "attometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0117, "femtometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0118, "femtometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0119, "picometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0120, "picometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0121, "gigametre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0122, "gigametre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0123, "terametre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0124, "terametre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0125, "caliber.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0126, "caliber.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0127, "point_adobe.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0128, "point_adobe.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0129, "milligram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0130, "milligram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0131, "centigram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0132, "centigram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0133, "decigram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0134, "decigram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0135, "gram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0136, "gram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0137, "decagram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0138, "decagram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0139, "hectogram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0140, "hectogram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0141, "tonne.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0142, "tonne.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0143, "kilotonne.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0144, "kilotonne.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0145, "megatonne.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0146, "megatonne.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0147, "microgram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0148, "microgram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0149, "ounce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0150, "ounce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0151, "pound.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0152, "pound.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0153, "quarter_mass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0154, "quarter_mass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0155, "hundredweight.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0156, "hundredweight.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0157, "longton.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0158, "longton.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0159, "shortton.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0160, "shortton.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0161, "grain.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0162, "grain.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0163, "dram.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0164, "dram.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0165, "troyounce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0166, "troyounce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0167, "troypound.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0168, "troypound.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0169, "pennyweight.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0170, "pennyweight.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0171, "carat.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0172, "carat.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0173, "atomicmassunit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0174, "atomicmassunit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0175, "electronmass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0176, "electronmass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0177, "protonmass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0178, "protonmass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0179, "planckmass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0180, "planckmass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0181, "solarmass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0182, "solarmass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0183, "earthmass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0184, "earthmass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0185, "jupitermass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0186, "jupitermass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0187, "firkin_mass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0188, "firkin_mass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0189, "quintal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0190, "quintal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0191, "kip.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0192, "kip.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0193, "scruple.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0194, "scruple.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0195, "obol.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0196, "obol.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0197, "mina.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0198, "mina.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0199, "talent.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0200, "talent.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0201, "shekel.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0202, "shekel.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0203, "keel.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0204, "keel.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0205, "wey.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0206, "wey.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0207, "tod.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0208, "tod.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0209, "clove.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0210, "clove.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0211, "mite_mass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0212, "mite_mass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0213, "bale.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0214, "bale.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0215, "millilitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0216, "millilitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0217, "centilitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0218, "centilitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0219, "decilitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0220, "decilitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0221, "decalitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0222, "decalitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0223, "hectolitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0224, "hectolitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0225, "kilolitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0226, "kilolitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0227, "microlitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0228, "microlitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0229, "cubicmetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0230, "cubicmetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0231, "cubiccentimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0232, "cubiccentimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0233, "cubicinch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0234, "cubicinch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0235, "cubicfoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0236, "cubicfoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0237, "cubicyard.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0238, "cubicyard.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0239, "gallon_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0240, "gallon_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0241, "gallon_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0242, "gallon_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0243, "quart_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0244, "quart_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0245, "quart_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0246, "quart_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0247, "pint_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0248, "pint_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0249, "pint_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0250, "pint_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0251, "cup_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0252, "cup_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0253, "cup_metric.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0254, "cup_metric.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0255, "fluidounce_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0256, "fluidounce_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0257, "fluidounce_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0258, "fluidounce_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0259, "tablespoon_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0260, "tablespoon_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0261, "teaspoon_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0262, "teaspoon_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0263, "gill_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0264, "gill_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0265, "gill_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0266, "gill_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0267, "minim_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0268, "minim_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0269, "dram_fluid.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0270, "dram_fluid.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0271, "barrel_oil.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0272, "barrel_oil.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0273, "barrel_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0274, "barrel_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0275, "barrel_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0276, "barrel_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0277, "hogshead.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0278, "hogshead.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0279, "firkin.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0280, "firkin.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0281, "kilderkin.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0282, "kilderkin.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0283, "peck_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0284, "peck_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0285, "bushel_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0286, "bushel_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0287, "bushel_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0288, "bushel_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0289, "peck_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0290, "peck_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0291, "cord.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0292, "cord.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0293, "boardfoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0294, "boardfoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0295, "acrefoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0296, "acrefoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0297, "jigger.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0298, "jigger.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0299, "shot_vol.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0300, "shot_vol.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0301, "pony.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0302, "pony.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0303, "magnum.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0304, "magnum.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0305, "jeroboam.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0306, "jeroboam.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0307, "methuselah.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0308, "methuselah.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0309, "nebuchadnezzar.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0310, "nebuchadnezzar.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0311, "tun.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0312, "tun.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0313, "butt_vol.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0314, "butt_vol.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0315, "puncheon.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0316, "puncheon.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0317, "stere.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0318, "stere.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0319, "lambda_vol.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0320, "lambda_vol.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0321, "drop_vol.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0322, "drop_vol.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0323, "bucket.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0324, "bucket.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0325, "pottle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0326, "pottle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0327, "noggin.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0328, "noggin.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0329, "quartern.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0330, "quartern.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0331, "squaremillimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0332, "squaremillimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0333, "squarecentimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0334, "squarecentimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0335, "squaredecimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0336, "squaredecimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0337, "squaremetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0338, "squaremetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0339, "squarekilometre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0340, "squarekilometre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0341, "squareinch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0342, "squareinch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0343, "squarefoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0344, "squarefoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0345, "squareyard.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0346, "squareyard.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0347, "squaremile.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0348, "squaremile.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0349, "acre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0350, "acre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0351, "hectare.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0352, "hectare.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0353, "are.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0354, "are.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0355, "centiare.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0356, "centiare.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0357, "rood_area.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0358, "rood_area.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0359, "squarerod.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0360, "squarerod.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0361, "squarechain.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0362, "squarechain.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0363, "barn.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0364, "barn.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0365, "township.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0366, "township.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0367, "section_area.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0368, "section_area.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0369, "squarenauticalmile.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0370, "squarenauticalmile.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0371, "dunam.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0372, "dunam.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0373, "squareperch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0374, "squareperch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0375, "squarelink.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0376, "squarelink.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0377, "circularmil.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0378, "circularmil.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0379, "cuerda.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0380, "cuerda.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0381, "morgen.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0382, "morgen.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0383, "carucate.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0384, "carucate.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0385, "virgate.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0386, "virgate.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0387, "hide.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0388, "hide.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0389, "millisecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0390, "millisecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0391, "microsecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0392, "microsecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0393, "nanosecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0394, "nanosecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0395, "picosecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0396, "picosecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0397, "minute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0398, "minute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0399, "hour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0400, "hour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0401, "day.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0402, "day.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0403, "week.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0404, "week.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0405, "fortnight.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0406, "fortnight.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0407, "month_avg.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0408, "month_avg.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0409, "year_julian.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0410, "year_julian.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0411, "year_common.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0412, "year_common.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0413, "decade.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0414, "decade.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0415, "century.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0416, "century.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0417, "millennium.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0418, "millennium.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0419, "jiffy_time.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0420, "jiffy_time.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0421, "shake.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0422, "shake.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0423, "planck_time.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0424, "planck_time.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0425, "sol_mars.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0426, "sol_mars.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0427, "lustrum.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0428, "lustrum.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0429, "olympiad.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0430, "olympiad.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0431, "ke_time.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0432, "ke_time.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0433, "ghurry.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0434, "ghurry.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0435, "watch_time.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0436, "watch_time.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0437, "blink.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0438, "blink.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0439, "moment_time.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0440, "moment_time.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0441, "sidereal_day.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0442, "sidereal_day.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0443, "sidereal_year.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0444, "sidereal_year.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0445, "anomalistic_year.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0446, "anomalistic_year.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0447, "kmh.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0448, "kmh.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0449, "mph.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0450, "mph.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0451, "knot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0452, "knot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0453, "footpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0454, "footpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0455, "machstd.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0456, "machstd.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0457, "speedoflight_u.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0458, "speedoflight_u.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0459, "inchpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0460, "inchpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0461, "kmpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0462, "kmpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0463, "milepersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0464, "milepersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0465, "footperminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0466, "footperminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0467, "metreperminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0468, "metreperminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0469, "furlongperfortnight.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0470, "furlongperfortnight.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0471, "beaufort_approx.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0472, "beaufort_approx.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0473, "yardpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0474, "yardpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0475, "pes_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0476, "pes_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0477, "gradus_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0478, "gradus_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0479, "passus_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0480, "passus_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0481, "actus_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0482, "actus_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0483, "milleromanum.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0484, "milleromanum.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0485, "verst.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0486, "verst.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0487, "arshin.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0488, "arshin.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0489, "sazhen.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0490, "sazhen.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0491, "vershok.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0492, "vershok.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0493, "li_chinese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0494, "li_chinese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0495, "zhang.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0496, "zhang.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0497, "chi_chinese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0498, "chi_chinese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0499, "cun_chinese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0500, "cun_chinese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0501, "ri_japanese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0502, "ri_japanese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0503, "ken_japanese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0504, "ken_japanese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0505, "shaku.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0506, "shaku.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0507, "sun_japanese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0508, "sun_japanese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0509, "wa_thai.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0510, "wa_thai.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0511, "braccio.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0512, "braccio.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0513, "canna.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0514, "canna.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0515, "toise.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0516, "toise.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0517, "aune.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0518, "aune.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0519, "pied_du_roi.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0520, "pied_du_roi.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0521, "vara.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0522, "vara.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0523, "legua.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0524, "legua.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0525, "libra_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0526, "libra_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0527, "uncia_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0528, "uncia_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0529, "catty.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0530, "catty.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0531, "tael.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0532, "tael.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0533, "momme.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0534, "momme.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0535, "picul.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0536, "picul.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0537, "maund.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0538, "maund.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0539, "seer.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0540, "seer.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0541, "tola.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0542, "tola.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0543, "candy_mass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0544, "candy_mass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0545, "oka.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0546, "oka.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0547, "rotl.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0548, "rotl.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0549, "pood.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0550, "pood.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0551, "zolotnik.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0552, "zolotnik.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0553, "funt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0554, "funt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0555, "koku.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0556, "koku.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0557, "to_japanese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0558, "to_japanese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0559, "sho.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0560, "sho.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0561, "go_japanese.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0562, "go_japanese.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0563, "amphora_roman.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0564, "amphora_roman.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0565, "congius.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0566, "congius.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0567, "sextarius.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0568, "sextarius.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0569, "cyathus.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0570, "cyathus.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0571, "modius.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0572, "modius.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0573, "urna.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0574, "urna.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0575, "oka_volume.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0576, "oka_volume.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0577, "seah.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0578, "seah.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0579, "hin.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0580, "hin.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0581, "omer.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0582, "omer.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0583, "ephah.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0584, "ephah.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0585, "gravitymoon.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0586, "gravitymoon.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0587, "gravitymars.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0588, "gravitymars.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0589, "gravityjupiter.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0590, "gravityjupiter.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0591, "gravityvenus.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0592, "gravityvenus.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0593, "gravitymercury.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0594, "gravitymercury.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0595, "gravitysaturn.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0596, "gravitysaturn.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0597, "gravityuranus.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0598, "gravityuranus.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0599, "gravityneptune.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0600, "gravityneptune.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0601, "gravitysun.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0602, "gravitysun.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0603, "gravitypluto.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0604, "gravitypluto.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0605, "gravityceres.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0606, "gravityceres.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0607, "gravitytitan.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0608, "gravitytitan.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0609, "gravityeuropa.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0610, "gravityeuropa.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0611, "gravityio.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0612, "gravityio.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0613, "gravityganymede.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0614, "gravityganymede.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0615, "kilopascal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0616, "kilopascal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0617, "megapascal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0618, "megapascal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0619, "gigapascal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0620, "gigapascal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0621, "hectopascal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0622, "hectopascal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0623, "bar.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0624, "bar.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0625, "millibar.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0626, "millibar.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0627, "microbar.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0628, "microbar.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0629, "atmosphere.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0630, "atmosphere.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0631, "torr.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0632, "torr.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0633, "mmhg.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0634, "mmhg.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0635, "inhg.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0636, "inhg.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0637, "psi.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0638, "psi.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0639, "ksi.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0640, "ksi.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0641, "mmwater.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0642, "mmwater.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0643, "inwater.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0644, "inwater.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0645, "barye.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0646, "barye.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0647, "pieze.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0648, "pieze.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0649, "technicalatmosphere.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0650, "technicalatmosphere.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0651, "kgfcm2.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0652, "kgfcm2.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0653, "dynecm2.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0654, "dynecm2.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0655, "footofwater.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0656, "footofwater.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0657, "poundalft2.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0658, "poundalft2.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0659, "kilojoule.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0660, "kilojoule.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0661, "megajoule.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0662, "megajoule.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0663, "gigajoule.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0664, "gigajoule.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0665, "millijoule.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0666, "millijoule.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0667, "microjoule.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0668, "microjoule.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0669, "calorie.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0670, "calorie.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0671, "kilocalorie.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0672, "kilocalorie.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0673, "watthour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0674, "watthour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0675, "kilowatthour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0676, "kilowatthour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0677, "megawatthour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0678, "megawatthour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0679, "electronvolt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0680, "electronvolt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0681, "kiloelectronvolt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0682, "kiloelectronvolt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0683, "megaelectronvolt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0684, "megaelectronvolt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0685, "erg.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0686, "erg.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0687, "britishthermalunit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0688, "britishthermalunit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0689, "therm.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0690, "therm.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0691, "footpound.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0692, "footpound.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0693, "footpoundal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0694, "footpoundal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0695, "tontnt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0696, "tontnt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0697, "kilotontnt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0698, "kilotontnt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0699, "megatontnt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0700, "megatontnt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0701, "hartree.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0702, "hartree.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0703, "rydberg_energy.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0704, "rydberg_energy.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0705, "quad.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0706, "quad.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0707, "horsepowerhour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0708, "horsepowerhour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0709, "litreatmosphere.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0710, "litreatmosphere.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0711, "celsiusheatunit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0712, "celsiusheatunit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0713, "kilowatt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0714, "kilowatt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0715, "megawatt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0716, "megawatt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0717, "gigawatt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0718, "gigawatt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0719, "milliwatt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0720, "milliwatt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0721, "microwatt.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0722, "microwatt.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0723, "horsepower_mech.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0724, "horsepower_mech.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0725, "horsepower_metric.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0726, "horsepower_metric.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0727, "horsepower_electric.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0728, "horsepower_electric.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0729, "btuperhour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0730, "btuperhour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0731, "caloriepersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0732, "caloriepersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0733, "ergpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0734, "ergpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0735, "footpoundpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0736, "footpoundpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0737, "tonrefrigeration.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0738, "tonrefrigeration.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0739, "voltampere.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0740, "voltampere.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0741, "kilovoltampere.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0742, "kilovoltampere.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0743, "pferdestarke.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0744, "pferdestarke.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0745, "poncelet.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0746, "poncelet.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0747, "lusec.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0748, "lusec.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0749, "kilonewton.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0750, "kilonewton.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0751, "meganewton.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0752, "meganewton.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0753, "millinewton.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0754, "millinewton.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0755, "dyne.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0756, "dyne.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0757, "kilogramforce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0758, "kilogramforce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0759, "gramforce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0760, "gramforce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0761, "poundforce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0762, "poundforce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0763, "ounceforce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0764, "ounceforce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0765, "poundal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0766, "poundal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0767, "kipforce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0768, "kipforce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0769, "tonneforce.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0770, "tonneforce.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0771, "sthene.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0772, "sthene.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0773, "crinal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0774, "crinal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0775, "bit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0776, "bit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0777, "kilobit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0778, "kilobit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0779, "megabit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0780, "megabit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0781, "gigabit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0782, "gigabit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0783, "kilobyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0784, "kilobyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0785, "megabyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0786, "megabyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0787, "gigabyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0788, "gigabyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0789, "terabyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0790, "terabyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0791, "petabyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0792, "petabyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0793, "exabyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0794, "exabyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0795, "kibibyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0796, "kibibyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0797, "mebibyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0798, "mebibyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0799, "gibibyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0800, "gibibyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0801, "tebibyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0802, "tebibyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0803, "pebibyte.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0804, "pebibyte.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0805, "kibibit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0806, "kibibit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0807, "mebibit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0808, "mebibit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0809, "word16.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0810, "word16.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0811, "dword.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0812, "dword.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0813, "qword.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0814, "qword.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0815, "octet.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0816, "octet.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0817, "crumb.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0818, "crumb.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0819, "radian.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0820, "radian.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0821, "gradian.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0822, "gradian.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0823, "arcminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0824, "arcminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0825, "arcsecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0826, "arcsecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0827, "milliradian.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0828, "milliradian.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0829, "turn_angle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0830, "turn_angle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0831, "quadrant.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0832, "quadrant.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0833, "sextant.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0834, "sextant.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0835, "octant_angle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0836, "octant_angle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0837, "sign_angle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0838, "sign_angle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0839, "point_compass.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0840, "point_compass.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0841, "milnato.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0842, "milnato.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0843, "milsoviet.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0844, "milsoviet.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0845, "hourangle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0846, "hourangle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0847, "centesimalminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0848, "centesimalminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0849, "centesimalsecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0850, "centesimalsecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0851, "binarydegree.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0852, "binarydegree.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0853, "kilohertz.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0854, "kilohertz.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0855, "megahertz.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0856, "megahertz.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0857, "gigahertz.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0858, "gigahertz.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0859, "terahertz.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0860, "terahertz.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0861, "millihertz.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0862, "millihertz.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0863, "rpm_freq.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0864, "rpm_freq.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0865, "radianpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0866, "radianpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0867, "cyclepersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0868, "cyclepersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0869, "bpm_freq.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0870, "bpm_freq.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0871, "phot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0872, "phot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0873, "nox.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0874, "nox.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0875, "footcandle.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0876, "footcandle.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0877, "lumenpersquaremetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0878, "lumenpersquaremetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0879, "kilolux.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0880, "kilolux.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0881, "milphot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0882, "milphot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0883, "millicoulomb.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0884, "millicoulomb.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0885, "microcoulomb.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0886, "microcoulomb.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0887, "nanocoulomb.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0888, "nanocoulomb.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0889, "abcoulomb.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0890, "abcoulomb.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0891, "statcoulomb.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0892, "statcoulomb.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0893, "amperehour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0894, "amperehour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0895, "milliamperehour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0896, "milliamperehour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0897, "faradayconstant_c.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0898, "faradayconstant_c.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0899, "elementarycharge.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0900, "elementarycharge.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0901, "franklin.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0902, "franklin.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0903, "grampercubiccentimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0904, "grampercubiccentimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0905, "kilogrampercubicmetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0906, "kilogrampercubicmetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0907, "gramperlitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0908, "gramperlitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0909, "poundpercubicfoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0910, "poundpercubicfoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0911, "poundpercubicinch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0912, "poundpercubicinch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0913, "ouncepercubicinch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0914, "ouncepercubicinch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0915, "poundpergallon_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0916, "poundpergallon_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0917, "poundpergallon_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0918, "poundpergallon_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0919, "slugpercubicfoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0920, "slugpercubicfoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0921, "tonnepercubicmetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0922, "tonnepercubicmetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0923, "litrepersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0924, "litrepersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0925, "litreperminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0926, "litreperminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0927, "litreperhour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0928, "litreperhour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0929, "cubicmetreperhour.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0930, "cubicmetreperhour.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0931, "gallonusperminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0932, "gallonusperminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0933, "cubicfootperminute.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0934, "cubicfootperminute.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0935, "cubicfootpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0936, "cubicfootpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0937, "barreloilperday.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0938, "barreloilperday.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0939, "newtoncentimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0940, "newtoncentimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0941, "kilonewtonmetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0942, "kilonewtonmetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0943, "poundforcefoot.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0944, "poundforcefoot.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0945, "poundforceinch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0946, "poundforceinch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0947, "ounceforceinch.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0948, "ounceforceinch.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0949, "kilogramforcemetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0950, "kilogramforcemetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0951, "dynecentimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0952, "dynecentimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0953, "gramforcecentimetre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0954, "gramforcecentimetre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0955, "gal_accel.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0956, "gal_accel.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0957, "standardgravity.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0958, "standardgravity.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0959, "footpersecondsquared.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0960, "footpersecondsquared.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0961, "inchpersecondsquared.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0962, "inchpersecondsquared.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0963, "kmperhourpersecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0964, "kmperhourpersecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0965, "milligal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0966, "milligal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0967, "milespergallon_us.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0968, "milespergallon_us.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0969, "milespergallon_imp.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0970, "milespergallon_imp.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0971, "litresper100km_inv.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0972, "litresper100km_inv.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0973, "kmperlitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0974, "kmperlitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0975, "milesperlitre.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0976, "milesperlitre.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0977, "millimole.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0978, "millimole.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0979, "micromole.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0980, "micromole.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0981, "kilomole.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0982, "kilomole.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0983, "nanomole.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0984, "nanomole.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0985, "poundmole.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0986, "poundmole.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0987, "katal.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0988, "katal.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0989, "enzymeunit.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0990, "enzymeunit.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0991, "kilobecquerel.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0992, "kilobecquerel.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0993, "megabecquerel.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0994, "megabecquerel.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0995, "gigabecquerel.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0996, "gigabecquerel.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0997, "curie.si", Sig::Fixed(&[R], Some(Real))),
    (Mk0998, "curie.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk0999, "millicurie.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1000, "millicurie.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1001, "microcurie.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1002, "microcurie.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1003, "rutherford.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1004, "rutherford.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1005, "gray_dose.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1006, "gray_dose.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1007, "rad_dose.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1008, "rad_dose.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1009, "rem_dose.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1010, "rem_dose.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1011, "sievert_dose.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1012, "sievert_dose.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1013, "roentgen.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1014, "roentgen.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1015, "banana_dose.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1016, "banana_dose.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1017, "poise.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1018, "poise.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1019, "centipoise.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1020, "centipoise.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1021, "millipascalsecond.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1022, "millipascalsecond.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1023, "stokes.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1024, "stokes.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1025, "centistokes.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1026, "centistokes.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1027, "reyn.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1028, "reyn.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1029, "gauss_mag.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1030, "gauss_mag.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1031, "milligauss.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1032, "milligauss.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1033, "microtesla.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1034, "microtesla.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1035, "millitesla.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1036, "millitesla.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1037, "maxwell_mag.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1038, "maxwell_mag.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1039, "weber_mag.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1040, "weber_mag.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1041, "gamma_mag.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1042, "gamma_mag.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1043, "didotpoint.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1044, "didotpoint.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1045, "cicero.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1046, "cicero.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1047, "agate.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1048, "agate.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1049, "em_typo.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1050, "em_typo.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1051, "en_typo.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1052, "en_typo.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1053, "dessertspoon.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1054, "dessertspoon.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1055, "coffeespoon.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1056, "coffeespoon.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1057, "dash_cook.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1058, "dash_cook.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1059, "pinch_cook.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1060, "pinch_cook.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1061, "smidgen.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1062, "smidgen.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1063, "drop_cook.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1064, "drop_cook.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1065, "wineglass_cook.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1066, "wineglass_cook.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1067, "teacup_cook.si", Sig::Fixed(&[R], Some(Real))),
    (Mk1068, "teacup_cook.inv", Sig::Fixed(&[R], Some(Real))),
    (Mk1069, "phi.value", Sig::Fixed(&[], Some(Real))),
    (Mk1070, "goldenangle.value", Sig::Fixed(&[], Some(Real))),
    (Mk1071, "silverratio.value", Sig::Fixed(&[], Some(Real))),
    (Mk1072, "plasticnumber.value", Sig::Fixed(&[], Some(Real))),
    (Mk1073, "supergolden.value", Sig::Fixed(&[], Some(Real))),
    (Mk1074, "tribonacciconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1075, "eulernumber.value", Sig::Fixed(&[], Some(Real))),
    (Mk1076, "pisquared.value", Sig::Fixed(&[], Some(Real))),
    (Mk1077, "tau_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1078, "sqrt2.value", Sig::Fixed(&[], Some(Real))),
    (Mk1079, "sqrt3.value", Sig::Fixed(&[], Some(Real))),
    (Mk1080, "sqrt5.value", Sig::Fixed(&[], Some(Real))),
    (Mk1081, "sqrt7.value", Sig::Fixed(&[], Some(Real))),
    (Mk1082, "cbrt2.value", Sig::Fixed(&[], Some(Real))),
    (Mk1083, "ln2.value", Sig::Fixed(&[], Some(Real))),
    (Mk1084, "ln10.value", Sig::Fixed(&[], Some(Real))),
    (Mk1085, "log2e.value", Sig::Fixed(&[], Some(Real))),
    (Mk1086, "log10e.value", Sig::Fixed(&[], Some(Real))),
    (Mk1087, "catalan.value", Sig::Fixed(&[], Some(Real))),
    (Mk1088, "apery.value", Sig::Fixed(&[], Some(Real))),
    (Mk1089, "eulergamma.value", Sig::Fixed(&[], Some(Real))),
    (Mk1090, "feigenbaumdelta.value", Sig::Fixed(&[], Some(Real))),
    (Mk1091, "feigenbaumalpha.value", Sig::Fixed(&[], Some(Real))),
    (Mk1092, "conwayconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1093, "khinchin.value", Sig::Fixed(&[], Some(Real))),
    (Mk1094, "glaisher.value", Sig::Fixed(&[], Some(Real))),
    (Mk1095, "millsconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1096, "gauss_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1097, "omegaconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1098, "gelfond.value", Sig::Fixed(&[], Some(Real))),
    (Mk1099, "gelfondschneider.value", Sig::Fixed(&[], Some(Real))),
    (Mk1100, "ramanujansoldner.value", Sig::Fixed(&[], Some(Real))),
    (Mk1101, "erdosborwein.value", Sig::Fixed(&[], Some(Real))),
    (Mk1102, "twinprime.value", Sig::Fixed(&[], Some(Real))),
    (Mk1103, "brun.value", Sig::Fixed(&[], Some(Real))),
    (Mk1104, "golombdickman.value", Sig::Fixed(&[], Some(Real))),
    (Mk1105, "cahen.value", Sig::Fixed(&[], Some(Real))),
    (Mk1106, "laplacelimit.value", Sig::Fixed(&[], Some(Real))),
    (Mk1107, "lemniscate.value", Sig::Fixed(&[], Some(Real))),
    (Mk1108, "wallis_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1109, "universalparabolic.value", Sig::Fixed(&[], Some(Real))),
    (Mk1110, "sierpinski_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1111, "backhouse.value", Sig::Fixed(&[], Some(Real))),
    (Mk1112, "porter.value", Sig::Fixed(&[], Some(Real))),
    (Mk1113, "lieb.value", Sig::Fixed(&[], Some(Real))),
    (Mk1114, "niven.value", Sig::Fixed(&[], Some(Real))),
    (Mk1115, "viswanath.value", Sig::Fixed(&[], Some(Real))),
    (Mk1116, "foiasconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1117, "deviccoconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1118, "somosquad.value", Sig::Fixed(&[], Some(Real))),
    (Mk1119, "speedoflight_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1120, "gravitationalconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1121, "planckconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1122, "reducedplanck.value", Sig::Fixed(&[], Some(Real))),
    (Mk1123, "boltzmann.value", Sig::Fixed(&[], Some(Real))),
    (Mk1124, "avogadro_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1125, "gasconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1126, "elementarycharge_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1127, "electronmass_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1128, "protonmass_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1129, "neutronmass.value", Sig::Fixed(&[], Some(Real))),
    (Mk1130, "faraday_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1131, "stefanboltzmann.value", Sig::Fixed(&[], Some(Real))),
    (Mk1132, "vacuumpermittivity.value", Sig::Fixed(&[], Some(Real))),
    (Mk1133, "vacuumpermeability.value", Sig::Fixed(&[], Some(Real))),
    (Mk1134, "finestructure.value", Sig::Fixed(&[], Some(Real))),
    (Mk1135, "rydbergconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1136, "bohrmagneton.value", Sig::Fixed(&[], Some(Real))),
    (Mk1137, "nuclearmagneton.value", Sig::Fixed(&[], Some(Real))),
    (Mk1138, "standardgravity_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1139, "atmosphere_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1140, "electronvolt_const.value", Sig::Fixed(&[], Some(Real))),
    (Mk1141, "hubbleconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1142, "solarconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1143, "earthgravity.value", Sig::Fixed(&[], Some(Real))),
    (Mk1144, "soundspeedair.value", Sig::Fixed(&[], Some(Real))),
    (Mk1145, "absolutezero_c.value", Sig::Fixed(&[], Some(Real))),
    (Mk1146, "molarvolume.value", Sig::Fixed(&[], Some(Real))),
    (Mk1147, "wienconstant.value", Sig::Fixed(&[], Some(Real))),
    (Mk1148, "josephsonconst.value", Sig::Fixed(&[], Some(Real))),
    (Mk1149, "vonklitzing.value", Sig::Fixed(&[], Some(Real))),
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
    "net", "gwgame", "grad", "gwframe", "gwsim", "synth", "symbol", "ai", "task",
    // batch M
    "methuselah", "yardpersecond", "kilderkin", "footofwater", "poise", "anomalistic_year", "sho", "squarerod", "sidereal_day", "picometre",
    "hectare", "avogadro_const", "zolotnik", "knot", "maund", "therm", "hin", "beaufort_approx", "erg", "cahen",
    "bar", "milespergallon_us", "squareinch", "rpm_freq", "gravitysaturn", "smidgen", "poundalft2", "cup_us", "shackle", "microlitre",
    "centesimalsecond", "sextant", "phot", "porter", "angstrom", "barrel_imp", "plasticnumber", "finger", "milleromanum", "picosecond",
    "slugpercubicfoot", "seah", "footcandle", "lightminute", "catalan", "stefanboltzmann", "gray_dose", "barrel_oil", "kmperlitre", "rad_dose",
    "sqrt7", "twip", "gradus_roman", "poundal", "millimetre", "year_julian", "quadrant", "bushel_imp", "microsecond", "inchpersecondsquared",
    "voltampere", "pica", "kibibit", "peck_us", "mmhg", "palm", "footpersecondsquared", "kmpersecond", "klick", "faraday_const",
    "microtesla", "microbar", "lieb", "mina", "wey", "kilobit", "centilitre", "speedoflight_const", "wienconstant", "libra_roman",
    "legua", "ouncepercubicinch", "passus_roman", "horsepower_metric", "poundpergallon_us", "gigabyte", "celsiusheatunit", "amperehour", "teaspoon_us", "decade",
    "sun_japanese", "electronmass", "li_chinese", "gigawatt", "moment_time", "pied_du_roi", "ounceforceinch", "neutronmass", "noggin", "gigahertz",
    "gravitationalconst", "hundredweight", "kiloelectronvolt", "cubicfootpersecond", "cicero", "speedoflight_u", "litrepersecond", "atmosphere_const", "shake", "squaremillimetre",
    "quart_imp", "protonmass", "oka", "milnato", "milsoviet", "are", "seer", "elementarycharge", "coffeespoon", "megatonne",
    "quintal", "gigametre", "boltzmann", "litreperminute", "vershok", "centigram", "dynecentimetre", "pisquared", "squarekilometre", "millicoulomb",
    "katal", "pes_roman", "jeroboam", "agate", "poundforce", "attometre", "funt", "micron", "ell", "tola",
    "machstd", "troypound", "minim_us", "squarefoot", "arcminute", "stere", "scruple", "cubicinch", "hide", "megahertz",
    "bucket", "em_typo", "cubicfoot", "dunam", "candy_mass", "wa_thai", "kibibyte", "gelfondschneider", "centimetre", "fluidounce_imp",
    "earthradius", "gramforce", "sol_mars", "sidereal_year", "modius", "protonmass_const", "torr", "pferdestarke", "milligal", "gauss_mag",
    "microjoule", "gigajoule", "vacuumpermittivity", "rydbergconst", "gravityjupiter", "nox", "sign_angle", "grampercubiccentimetre", "megabecquerel", "millihertz",
    "statcoulomb", "shekel", "kilojoule", "plancklength", "rod", "quarter_mass", "tod", "dynecm2", "microcoulomb", "mil",
    "megapascal", "teacup_cook", "twinprime", "binarydegree", "word16", "feigenbaumalpha", "tonrefrigeration", "drop_vol", "squaredecimetre", "gigabecquerel",
    "sqrt3", "franklin", "barn", "ksi", "firkin_mass", "en_typo", "hectolitre", "turn_angle", "blink", "megajoule",
    "gallonusperminute", "centesimalminute", "inhg", "fathom", "kilomole", "brun", "barye", "kilometre", "jigger", "tebibyte",
    "finestructure", "pint_imp", "footpersecond", "ephah", "barleycorn", "lumenpersquaremetre", "shortton", "decilitre", "link", "urna",
    "gravityganymede", "gill_imp", "rood", "furlongperfortnight", "hectogram", "acrefoot", "technicalatmosphere", "exabyte", "millicurie", "dash_cook",
    "gravityio", "tun", "rem_dose", "thou", "newtoncentimetre", "glaisher", "microwatt", "drop_cook", "carucate", "shaku",
    "perch", "virgate", "gigabit", "furlong", "gravityeuropa", "stadium", "meganewton", "megabit", "poundforceinch", "dword",
    "megawatt", "sthene", "elementarycharge_const", "cup_metric", "chi_chinese", "pole", "olympiad", "megatontnt", "kip", "kilogramforce",
    "calorie", "tonnepercubicmetre", "acre", "boardfoot", "fortnight", "kilovoltampere", "decigram", "nanocoulomb", "grain", "canna",
    "standardgravity", "hubbleconst", "gasconstant", "sievert_dose", "radianpersecond", "gal_accel", "hartree", "peck_imp", "gravityuranus", "laplacelimit",
    "dram_fluid", "reyn", "femtometre", "supergolden", "digit", "arshin", "jiffy_time", "hourangle", "kilowatt", "mmwater",
    "millipascalsecond", "aune", "amphora_roman", "tribonacciconstant", "nauticalmile", "kgfcm2", "planckconst", "vacuumpermeability", "century", "vara",
    "electronvolt", "typographicpoint", "actus_roman", "gravitypluto", "pottle", "litreperhour", "cubicfootperminute", "hogshead", "milligauss", "gamma_mag",
    "radian", "watthour", "squarelink", "toise", "mebibyte", "jupitermass", "obol", "horsepower_mech", "centistokes", "poundpercubicfoot",
    "horsepower_electric", "footperminute", "milliwatt", "fermi", "eulernumber", "pood", "watch_time", "quart_us", "kilohertz", "magnum",
    "gelfond", "braccio", "roentgen", "squaremile", "sazhen", "cun_chinese", "omer", "horsepowerhour", "enzymeunit", "vonklitzing",
    "squarechain", "barreloilperday", "backhouse", "hour", "gauss_const", "decametre", "crinal", "zhang", "poundmole", "planckmass",
    "gramforcecentimetre", "day", "bit", "octet", "nail", "poundforcefoot", "millsconstant", "circularmil", "millilitre", "league",
    "dessertspoon", "kilocalorie", "firkin", "log10e", "cubicmetre", "wallis_const", "millimole", "qword", "pennyweight", "decalitre",
    "decagram", "lightsecond", "megaelectronvolt", "ln10", "eulergamma", "squareyard", "arcsecond", "cubicyard", "bushel_us", "quad",
    "ke_time", "tablespoon_us", "decimetre", "footpoundal", "catty", "go_japanese", "tael", "gallon_imp", "phi", "puncheon",
    "week", "absolutezero_c", "point_compass", "ken_japanese", "nuclearmagneton", "sqrt5", "gravityneptune", "squareperch", "nanosecond", "lunardistance",
    "molarvolume", "millinewton", "bpm_freq", "soundspeedair", "cyclepersecond", "butt_vol", "foot", "cubit", "milphot", "ln2",
    "atomicmassunit", "gramperlitre", "goldenangle", "tonne", "wineglass_cook", "rood_area", "year_common", "bohrmagneton", "viswanath", "didotpoint",
    "poundpergallon_imp", "terabyte", "milliamperehour", "banana_dose", "caliber", "mph", "centipoise", "lemniscate", "poncelet", "micromole",
    "lambda_vol", "atmosphere", "erdosborwein", "section_area", "deviccoconst", "lusec", "golombdickman", "bohrradius", "sierpinski_const", "footpound",
    "sqrt2", "verst", "kilolitre", "gravitysun", "kipforce", "pound", "momme", "smoot", "footpoundpersecond", "kilonewton",
    "longton", "milesperlitre", "kilonewtonmetre", "cbrt2", "chain", "standardgravity_const", "btuperhour", "koku", "inch", "litreatmosphere",
    "feigenbaumdelta", "month_avg", "pieze", "gravityvenus", "microcurie", "caloriepersecond", "planck_time", "millisecond", "ounce", "milligram",
    "kmh", "conwayconstant", "tau_const", "milespergallon_imp", "microgram", "bale", "terahertz", "apery", "curie", "to_japanese",
    "kilobyte", "log2e", "poundpercubicinch", "megabyte", "ri_japanese", "morgen", "rydberg_energy", "picul", "electronmass_const", "omegaconstant",
    "pebibyte", "ounceforce", "gravityceres", "pinch_cook", "gigapascal", "kmperhourpersecond", "nebuchadnezzar", "troyounce", "millibar", "kilogramforcemetre",
    "keel", "mite_mass", "township", "kilowatthour", "universalparabolic", "millennium", "ghurry", "gradian", "rutherford", "mile",
    "kilopascal", "gravitymercury", "gram", "uncia_roman", "octant_angle", "fluidounce_us", "pint_us", "astronomicalunit", "tonneforce", "hectopascal",
    "kilogrampercubicmetre", "reducedplanck", "cubiccentimetre", "cuerda", "oka_volume", "hand", "megametre", "milepersecond", "niven", "dyne",
    "gallon_us", "megawatthour", "congius", "inwater", "foiasconst", "lustrum", "solarradius", "minute", "rotl", "weber_mag",
    "gravitymoon", "solarconstant", "petabyte", "faradayconstant_c", "nanometre", "stokes", "point_adobe", "cable", "squarecentimetre", "kilobecquerel",
    "myriametre", "litresper100km_inv", "yard", "hectometre", "terametre", "milliradian", "earthgravity", "carat", "metreperminute", "squaremetre",
    "maxwell_mag", "ergpersecond", "electronvolt_const", "xunit", "cyathus", "clove", "gravitytitan", "khinchin", "parsec", "earthmass",
    "pony", "gibibyte", "josephsonconst", "barrel_us", "cubicmetreperhour", "span", "kilotonne", "somosquad", "talent", "millijoule",
    "lightyear", "inchpersecond", "gravitymars", "squarenauticalmile", "quartern", "britishthermalunit", "abcoulomb", "mebibit", "solarmass", "centiare",
    "dram", "tontnt", "millitesla", "nanomole", "cord", "rope", "kilotontnt", "shot_vol", "psi", "kilolux",
    "ramanujansoldner", "gill_us", "silverratio", "sextarius", "crumb",
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
            GwgKey => Value::Bool(self.window()?.key(text(&args[0]))?),
            GwgAxis => {
                let neg = self.window()?.key(text(&args[0]))?;
                let pos = self.window()?.key(text(&args[1]))?;
                Value::Int(pos as i64 - neg as i64)
            }
            GwgSprite => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let c = parse_color(text(&args[4]))?;
                self.window()?.canvas.rect(x, y, w, h, c);
                Value::Null
            }
            GwgTile => {
                let (x, y, s) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let c = parse_color(text(&args[3]))?;
                self.window()?.canvas.rect(x, y, s, s, c);
                Value::Null
            }
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

            // ---- gtq batch G: liquid glass ----
            GtqGlass => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let tint = parse_color(text(&args[4]))?;
                let blur = int(&args[5]);
                let radius = (w.min(h) * 0.22).min(28.0);
                self.window()?.canvas.glass(x, y, w, h, tint, blur, radius);
                Value::Null
            }
            GtqGlassRound => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let tint = parse_color(text(&args[4]))?;
                let blur = int(&args[5]);
                let radius = real(&args[6]);
                self.window()?.canvas.glass(x, y, w, h, tint, blur, radius);
                Value::Null
            }
            GtqGlassCircle => {
                let (x, y, r) = (real(&args[0]), real(&args[1]), real(&args[2]));
                let tint = parse_color(text(&args[3]))?;
                let blur = int(&args[4]);
                self.window()?.canvas.glass(x, y, 2.0 * r, 2.0 * r, tint, blur, r);
                Value::Null
            }
            GtqGlassButton => {
                let (x, y, w, h) = (real(&args[0]), real(&args[1]), real(&args[2]), real(&args[3]));
                let label = text(&args[4]).to_string();
                let (hover, down) = {
                    let g = self.graphics.as_ref().ok_or_else(|| NO_WINDOW.to_string())?;
                    let (mx, my) = g.mouse();
                    let (mx, my) = (mx as f64, my as f64);
                    (mx >= x && mx <= x + w && my >= y && my <= y + h, g.click())
                };
                let pressed = hover && down && !self.prev_down;
                let tint = parse_color(if hover { "#ffffff" } else { "#cbd5e1" })?;
                let txtc = parse_color("#1e293b")?;
                let radius = (h * 0.5).min(24.0);
                let tw = self.fonts.width(&label).unwrap_or(0) as f64;
                let g = self.graphics.as_mut().ok_or_else(|| NO_WINDOW.to_string())?;
                g.canvas.glass(x, y, w, h, tint, 6, radius);
                self.fonts.draw(&mut g.canvas, (x + (w - tw) / 2.0).max(x + 2.0), y + h / 2.0 - 6.0, &label, txtc)?;
                Value::Bool(pressed)
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
