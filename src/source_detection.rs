// src/source_detection.rs

use once_cell::sync::Lazy;
use std::collections::HashSet;
use std::path::Path;

// Using Lazy for zero-cost initialization of our static set
static SOURCE_EXTENSIONS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut set = HashSet::new();

    set.extend(&[
        "1",  // Roff, Roff Manpage
        "1in",  // Roff, Roff Manpage
        "1m",  // Roff, Roff Manpage
        "1x",  // Roff, Roff Manpage
        "2",  // Roff, Roff Manpage
        "2da",  // 2-Dimensional Array
        "3",  // Roff, Roff Manpage
        "3in",  // Roff, Roff Manpage
        "3m",  // Roff, Roff Manpage
        "3p",  // Roff, Roff Manpage
        "3pm",  // Roff, Roff Manpage
        "3qt",  // Roff, Roff Manpage
        "3x",  // Roff, Roff Manpage
        "4",  // Roff, Roff Manpage
        "4DForm",  // JSON
        "4DProject",  // JSON
        "4dm",  // 4D
        "4gl",  // Genero 4gl
        "4th",  // Forth
        "5",  // Roff, Roff Manpage
        "6",  // Roff, Roff Manpage
        "6pl",  // Raku
        "6pm",  // Raku
        "7",  // Roff, Roff Manpage
        "8",  // Roff, Roff Manpage
        "8xp",  // TI Program
        "8xp.txt",  // TI Program
        "9",  // Roff, Roff Manpage
        "Dsr",  // Visual Basic 6.0
        "JSON-tmLanguage",  // JSON
        "OutJob",  // Altium Designer
        "PcbDoc",  // Altium Designer
        "PrjPCB",  // Altium Designer
        "SchDoc",  // Altium Designer
        "TextGrid",  // TextGrid
        "_coffee",  // CoffeeScript
        "_js",  // JavaScript
        "_ls",  // LiveScript
        "a51",  // Assembly
        "abap",  // ABAP
        "abnf",  // ABNF
        "ada",  // Ada
        "adb",  // Ada
        "adml",  // XML
        "admx",  // XML
        "ado",  // Stata
        "adoc",  // AsciiDoc
        "adp",  // Tcl
        "ads",  // Ada
        "afm",  // Adobe Font Metrics
        "agc",  // Apollo Guidance Computer
        "agda",  // Agda
        "ahk",  // AutoHotkey
        "ahkl",  // AutoHotkey
        "aidl",  // AIDL
        "aj",  // AspectJ
        "al",  // AL, Perl
        "als",  // Alloy
        "ampl",  // AMPL
        "angelscript",  // AngelScript
        "anim",  // Unity3D Asset
        "ant",  // XML
        "antlers.html",  // Antlers
        "antlers.php",  // Antlers
        "antlers.xml",  // Antlers
        "apacheconf",  // ApacheConf
        "apib",  // API Blueprint
        "apl",  // APL
        "app",  // Erlang
        "app.src",  // Erlang
        "applescript",  // AppleScript
        "arc",  // Arc
        "arpa",  // DNS Zone
        "arr",  // Pyret
        "as",  // ActionScript, AngelScript
        "asax",  // ASP.NET
        "asc",  // AGS Script, AsciiDoc, Public Key
        "asciidoc",  // AsciiDoc
        "ascx",  // ASP.NET
        "asd",  // Common Lisp
        "asddls",  // ABAP CDS
        "ash",  // AGS Script
        "ashx",  // ASP.NET
        "asl",  // ASL
        "asm",  // Assembly, Motorola 68K Assembly
        "asmx",  // ASP.NET
        "asn",  // ASN.1
        "asn1",  // ASN.1
        "asp",  // Classic ASP
        "aspx",  // ASP.NET
        "asset",  // Unity3D Asset
        "astro",  // Astro
        "asy",  // Asymptote, LTspice Symbol
        "au3",  // AutoIt
        "aug",  // Augeas
        "auk",  // Awk
        "aux",  // TeX
        "avdl",  // Avro IDL
        "avsc",  // JSON
        "aw",  // PHP
        "awk",  // Awk
        "axaml",  // XML
        "axd",  // ASP.NET
        "axi",  // NetLinx
        "axi.erb",  // NetLinx+ERB
        "axml",  // XML
        "axs",  // NetLinx
        "axs.erb",  // NetLinx+ERB
        "b",  // Brainfuck, Limbo
        "bal",  // Ballerina
        "bas",  // B4X, BASIC, FreeBASIC, ...
        "bash",  // Shell
        "bat",  // Batchfile
        "bats",  // Shell
        "bb",  // BitBake, BlitzBasic, Clojure
        "bbappend",  // BitBake
        "bbclass",  // BitBake
        "bbx",  // TeX
        "bdf",  // Glyph Bitmap Distribution Format
        "bdy",  // PLSQL
        "be",  // Berry
        "befunge",  // Befunge
        "bf",  // Beef, Befunge, Brainfuck, ...
        "bi",  // FreeBASIC
        "bib",  // BibTeX
        "bibtex",  // BibTeX
        "bicep",  // Bicep
        "bicepparam",  // Bicep
        "bison",  // Bison
        "blade",  // Blade
        "blade.php",  // Blade
        "bmx",  // BlitzMax
        "bones",  // JavaScript
        "boo",  // Boo
        "boot",  // Clojure
        "bpl",  // Boogie
        "bqn",  // BQN
        "brd",  // Eagle, KiCad Legacy Layout
        "bro",  // Zeek
        "brs",  // Brightscript
        "bs",  // Bikeshed, Bluespec BH, BrighterScript
        "bsl",  // 1C Enterprise
        "bsv",  // Bluespec
        "builder",  // Ruby
        "builds",  // XML
        "bzl",  // Starlark
        "c",  // C
        "c++",  // C++
        "c++-objdump",  // Cpp-ObjDump
        "c++objdump",  // Cpp-ObjDump
        "c-objdump",  // C-ObjDump
        "cabal",  // Cabal Config
        "caddyfile",  // Caddyfile
        "cairo",  // Cairo, Cairo Zero
        "cake",  // C#, CoffeeScript
        "capnp",  // Cap'n Proto
        "carbon",  // Carbon
        "cats",  // C
        "cbl",  // COBOL
        "cbx",  // TeX
        "cc",  // C++
        "ccp",  // COBOL
        "ccproj",  // XML
        "ccxml",  // XML
        "cdc",  // Cadence
        "cdf",  // Mathematica
        "cds",  // CAP CDS
        "ceylon",  // Ceylon
        "cfc",  // ColdFusion CFC
        "cfg",  // HAProxy, INI
        "cfm",  // ColdFusion
        "cfml",  // ColdFusion
        "cgi",  // Perl, Python, Shell
        "cginc",  // HLSL
        "ch",  // Charity, xBase
        "chem",  // Pic
        "chpl",  // Chapel
        "chs",  // C2hs Haskell
        "cil",  // CIL
        "circom",  // Circom
        "cirru",  // Cirru
        "cjs",  // JavaScript
        "cjsx",  // CoffeeScript
        "ck",  // ChucK
        "cl",  // Common Lisp, Cool, OpenCL
        "cl2",  // Clojure
        "clar",  // Clarity
        "click",  // Click
        "clixml",  // XML
        "clj",  // Clojure
        "cljc",  // Clojure
        "cljs",  // Clojure
        "cljs.hl",  // Clojure
        "cljscm",  // Clojure
        "cljx",  // Clojure
        "clp",  // CLIPS
        "cls",  // Apex, ObjectScript, OpenEdge ABL, ...
        "clw",  // Clarion
        "cmake",  // CMake
        "cmake.in",  // CMake
        "cmd",  // Batchfile
        "cmp",  // Gerber Image
        "cnc",  // G-code
        "cnf",  // INI
        "cob",  // COBOL
        "cobol",  // COBOL
        "cocci",  // SmPL
        "code-snippets",  // JSON with Comments
        "code-workspace",  // JSON with Comments
        "coffee",  // CoffeeScript
        "coffee.md",  // Literate CoffeeScript
        "com",  // DIGITAL Command Language
        "command",  // Shell
        "conll",  // CoNLL-U
        "conllu",  // CoNLL-U
        "containerfile",  // Dockerfile
        "coq",  // Coq
        "cp",  // C++, Component Pascal
        "cpp",  // C++
        "cpp-objdump",  // Cpp-ObjDump
        "cppm",  // C++
        "cppobjdump",  // Cpp-ObjDump
        "cproject",  // XML
        "cps",  // Component Pascal
        "cpy",  // COBOL
        "cql",  // SQL
        "cr",  // Crystal
        "crc32",  // Checksums
        "creole",  // Creole
        "cs",  // C#, Smalltalk
        "cs.pp",  // C#
        "csc",  // GSC
        "cscfg",  // XML
        "csd",  // Csound Document
        "csdef",  // XML
        "csh",  // Tcsh
        "cshtml",  // HTML+Razor
        "csl",  // Kusto, XML
        "cson",  // CSON
        "csproj",  // XML
        "css",  // CSS
        "csv",  // CSV
        "csx",  // C#
        "ct",  // XML
        "ctl",  // Visual Basic 6.0
        "ctp",  // PHP
        "cts",  // TypeScript
        "cu",  // Cuda
        "cue",  // CUE, Cue Sheet
        "cuh",  // Cuda
        "curry",  // Curry
        "cw",  // Redcode
        "cwl",  // Common Workflow Language
        "cxx",  // C++
        "cxx-objdump",  // Cpp-ObjDump
        "cy",  // Cycript
        "cylc",  // Cylc
        "cyp",  // Cypher
        "cypher",  // Cypher
        "d",  // D, DTrace, Makefile
        "d-objdump",  // D-ObjDump
        "d2",  // D2
        "dae",  // COLLADA
        "darcspatch",  // Darcs Patch
        "dart",  // Dart
        "dats",  // ATS
        "db2",  // SQLPL
        "dcl",  // Clean
        "ddl",  // PLSQL, SQL
        "decls",  // BlitzBasic
        "depproj",  // XML
        "desktop",  // desktop
        "desktop.in",  // desktop
        "dfm",  // Pascal
        "dfy",  // Dafny
        "dhall",  // Dhall
        "di",  // D
        "diff",  // Diff
        "dircolors",  // dircolors
        "dita",  // XML
        "ditamap",  // XML
        "ditaval",  // XML
        "djs",  // Dogescript
        "dll.config",  // XML
        "dlm",  // IDL
        "dm",  // DM
        "do",  // Stata
        "dockerfile",  // Dockerfile
        "dof",  // INI
        "doh",  // Stata
        "dot",  // Graphviz (DOT)
        "dotsettings",  // XML
        "dpatch",  // Darcs Patch
        "dpr",  // Pascal
        "druby",  // Mirah
        "dsc",  // Debian Package Control File, DenizenScript
        "dsl",  // ASL
        "dsp",  // Faust, Microsoft Developer Studio Project
        "dtx",  // TeX
        "duby",  // Mirah
        "dwl",  // DataWeave
        "dyalog",  // APL
        "dyl",  // Dylan
        "dylan",  // Dylan
        "e",  // E, Eiffel, Euphoria
        "eam.fs",  // Formatted
        "eb",  // Easybuild
        "ebnf",  // EBNF
        "ebuild",  // Gentoo Ebuild
        "ec",  // eC
        "ecl",  // ECL, ECLiPSe
        "eclass",  // Gentoo Eclass
        "eclxml",  // ECL
        "ecr",  // HTML+ECR
        "ect",  // EJS
        "edc",  // Edje Data Collection
        "edge",  // Edge
        "edgeql",  // EdgeQL
        "editorconfig",  // EditorConfig
        "edn",  // edn
        "eex",  // HTML+EEX
        "eh",  // eC
        "ejs",  // EJS
        "ejs.t",  // EJS
        "el",  // Emacs Lisp
        "eliom",  // OCaml
        "eliomi",  // OCaml
        "elm",  // Elm
        "elv",  // Elvish
        "em",  // EmberScript
        "emacs",  // Emacs Lisp
        "emacs.desktop",  // Emacs Lisp
        "emberscript",  // EmberScript
        "eml",  // E-mail
        "epj",  // Ecere Projects
        "eps",  // PostScript
        "epsi",  // PostScript
        "eq",  // EQ
        "erb",  // HTML+ERB
        "erb.deface",  // HTML+ERB
        "erl",  // Erlang
        "es",  // Erlang, JavaScript
        "es6",  // JavaScript
        "escript",  // Erlang
        "esdl",  // EdgeQL
        "ex",  // Elixir, Euphoria
        "exs",  // Elixir
        "eye",  // Ruby
        "f",  // Filebench WML, Forth, Fortran
        "f03",  // Fortran Free Form
        "f08",  // Fortran Free Form
        "f77",  // Fortran
        "f90",  // Fortran Free Form
        "f95",  // Fortran Free Form
        "factor",  // Factor
        "fan",  // Fantom
        "fancypack",  // Fancy
        "fcgi",  // Lua, PHP, Perl, ...
        "fea",  // OpenType Feature File
        "feature",  // Gherkin
        "filters",  // XML
        "fir",  // FIRRTL
        "fish",  // fish
        "flex",  // JFlex
        "flf",  // FIGlet Font
        "flux",  // FLUX
        "fnc",  // PLSQL
        "fnl",  // Fennel
        "for",  // Formatted, Forth, Fortran
        "forth",  // Forth
        "fp",  // GLSL
        "fpp",  // Fortran
        "fr",  // Forth, Frege, Text
        "frag",  // GLSL, JavaScript
        "frg",  // GLSL
        "frm",  // VBA, Visual Basic 6.0
        "frt",  // Forth
        "fs",  // F#, Filterscript, Forth, ...
        "fsh",  // GLSL
        "fshader",  // GLSL
        "fsi",  // F#
        "fsproj",  // XML
        "fst",  // F*
        "fsti",  // F*
        "fsx",  // F#
        "fth",  // Forth
        "ftl",  // Fluent, FreeMarker
        "fun",  // Standard ML
        "fut",  // Futhark
        "fx",  // FLUX, HLSL
        "fxh",  // HLSL
        "fxml",  // XML
        "fy",  // Fancy
        "g",  // G-code, GAP
        "g4",  // ANTLR
        "gaml",  // GAML
        "gap",  // GAP
        "gawk",  // Awk
        "gbl",  // Gerber Image
        "gbo",  // Gerber Image
        "gbp",  // Gerber Image
        "gbr",  // Gerber Image
        "gbs",  // Gerber Image
        "gco",  // G-code
        "gcode",  // G-code
        "gd",  // GAP, GDScript
        "gdb",  // GDB
        "gdbinit",  // GDB
        "gdnlib",  // Godot Resource
        "gdns",  // Godot Resource
        "ged",  // GEDCOM
        "gemspec",  // Ruby
        "geo",  // GLSL
        "geojson",  // JSON
        "geom",  // GLSL
        "gf",  // Grammatical Framework
        "gi",  // GAP
        "gitignore",  // Ignore List
        "gjs",  // Glimmer JS
        "gko",  // Gerber Image
        "glade",  // XML
        "gleam",  // Gleam
        "glf",  // Glyph
        "glsl",  // GLSL
        "glslf",  // GLSL
        "glslv",  // GLSL
        "gltf",  // JSON
        "glyphs",  // OpenStep Property List
        "gmi",  // Gemini
        "gml",  // Game Maker Language, Gerber Image, Graph Modeling Language, ...
        "gms",  // GAMS
        "gmx",  // XML
        "gn",  // GN
        "gni",  // GN
        "gnu",  // Gnuplot
        "gnuplot",  // Gnuplot
        "go",  // Go
        "god",  // Ruby
        "golo",  // Golo
        "gp",  // Gnuplot
        "gpb",  // Gerber Image
        "gpt",  // Gerber Image
        "gql",  // GraphQL
        "grace",  // Grace
        "gradle",  // Gradle
        "gradle.kts",  // Gradle Kotlin DSL
        "graphql",  // GraphQL
        "graphqls",  // GraphQL
        "groovy",  // Groovy
        "grt",  // Groovy
        "grxml",  // XML
        "gs",  // GLSL, Genie, Gosu, ...
        "gsc",  // GSC
        "gsh",  // GSC
        "gshader",  // GLSL
        "gsp",  // Groovy Server Pages
        "gst",  // Gosu, XML
        "gsx",  // Gosu
        "gtl",  // Gerber Image
        "gto",  // Gerber Image
        "gtp",  // Gerber Image
        "gtpl",  // Groovy
        "gts",  // Gerber Image, Glimmer TS
        "gv",  // Graphviz (DOT)
        "gvy",  // Groovy
        "gyp",  // Python
        "gypi",  // Python
        "h",  // C, C++, Objective-C
        "h++",  // C++
        "ha",  // Hare
        "hack",  // Hack
        "haml",  // Haml
        "haml.deface",  // Haml
        "handlebars",  // Handlebars
        "har",  // JSON
        "hats",  // ATS
        "hb",  // Harbour
        "hbs",  // Handlebars
        "hc",  // HolyC
        "hcl",  // HCL
        "hh",  // C++, Hack
        "hhi",  // Hack
        "hic",  // Clojure
        "hlean",  // Lean
        "hlsl",  // HLSL
        "hlsli",  // HLSL
        "hocon",  // HOCON
        "hoon",  // hoon
        "hpp",  // C++
        "hqf",  // SQF
        "hql",  // HiveQL
        "hrl",  // Erlang
        "hs",  // Haskell
        "hs-boot",  // Haskell
        "hsc",  // Haskell
        "hta",  // HTML
        "htm",  // HTML
        "html",  // Ecmarkup, HTML
        "html.heex",  // HTML+EEX
        "html.hl",  // HTML
        "html.leex",  // HTML+EEX
        "http",  // HTTP
        "hx",  // Haxe
        "hxml",  // HXML
        "hxsl",  // Haxe
        "hxx",  // C++
        "hy",  // Hy
        "hzp",  // XML
        "i",  // Assembly, Motorola 68K Assembly, SWIG
        "i3",  // Modula-3
        "i7x",  // Inform 7
        "ical",  // iCalendar
        "ice",  // JSON, Slice
        "iced",  // CoffeeScript
        "icl",  // Clean
        "ics",  // iCalendar
        "idc",  // C
        "idr",  // Idris
        "ig",  // Modula-3
        "ihlp",  // Stata
        "ijm",  // ImageJ Macro
        "ijs",  // J
        "ik",  // Ioke
        "ily",  // LilyPond
        "imba",  // Imba
        "iml",  // XML
        "inc",  // Assembly, BitBake, C++, ...
        "ini",  // INI
        "ink",  // Ink
        "inl",  // C++
        "ino",  // C++
        "ins",  // TeX
        "intr",  // Dylan
        "io",  // Io
        "iol",  // Jolie
        "ipf",  // IGOR Pro
        "ipp",  // C++
        "ipynb",  // Jupyter Notebook
        "irclog",  // IRC log
        "isl",  // Inno Setup
        "iss",  // Inno Setup
        "iuml",  // PlantUML
        "ivy",  // XML
        "ixx",  // C++
        "j",  // Jasmin, Objective-J
        "j2",  // Jinja
        "jade",  // Pug
        "jake",  // JavaScript
        "janet",  // Janet
        "jav",  // Java
        "java",  // Java
        "javascript",  // JavaScript
        "jbuilder",  // Ruby
        "jcl",  // JCL
        "jelly",  // XML
        "jflex",  // JFlex
        "jinja",  // Jinja
        "jinja2",  // Jinja
        "jison",  // Jison
        "jisonlex",  // Jison Lex
        "jl",  // Julia
        "jq",  // JSONiq, jq
        "js",  // JavaScript
        "js.erb",  // JavaScript+ERB
        "jsb",  // JavaScript
        "jscad",  // JavaScript
        "jsfl",  // JavaScript
        "jsh",  // Java
        "jslib",  // JavaScript
        "jsm",  // JavaScript
        "json",  // JSON, OASv2-json, OASv3-json
        "json.example",  // JSON
        "json5",  // JSON5
        "jsonc",  // JSON with Comments
        "jsonl",  // JSON
        "jsonld",  // JSONLD
        "jsonnet",  // Jsonnet
        "jsp",  // Java Server Pages
        "jspre",  // JavaScript
        "jsproj",  // XML
        "jss",  // JavaScript
        "jst",  // EJS
        "jsx",  // JavaScript
        "jte",  // Java Template Engine
        "just",  // Just
        "kak",  // KakouneScript
        "kicad_mod",  // KiCad Layout
        "kicad_pcb",  // KiCad Layout
        "kicad_sch",  // KiCad Schematic
        "kicad_wks",  // KiCad Layout
        "kid",  // Genshi
        "kit",  // Kit
        "kml",  // XML
        "kojo",  // Scala
        "kql",  // Kusto
        "krl",  // KRL
        "ks",  // KerboScript, Kickstart
        "ksh",  // Shell
        "ksy",  // Kaitai Struct
        "kt",  // Kotlin
        "ktm",  // Kotlin
        "kts",  // Kotlin
        "kv",  // kvlang
        "l",  // Common Lisp, Lex, PicoLisp, ...
        "lagda",  // Literate Agda
        "lark",  // Lark
        "las",  // Lasso
        "lasso",  // Lasso
        "lasso8",  // Lasso
        "lasso9",  // Lasso
        "latte",  // Latte
        "launch",  // XML
        "lbx",  // TeX
        "ld",  // Linker Script
        "lds",  // Linker Script
        "lean",  // Lean, Lean 4
        "lektorproject",  // INI
        "less",  // Less
        "lex",  // Lex
        "lfe",  // LFE
        "lgt",  // Logtalk
        "lhs",  // Literate Haskell
        "libsonnet",  // Jsonnet
        "lid",  // Dylan
        "lidr",  // Idris
        "ligo",  // LigoLANG
        "linq",  // C#
        "liquid",  // Liquid
        "lisp",  // Common Lisp, NewLisp
        "litcoffee",  // Literate CoffeeScript
        "livecodescript",  // LiveCode Script
        "livemd",  // Markdown
        "lkml",  // LookML
        "ll",  // LLVM
        "lmi",  // Python
        "logtalk",  // Logtalk
        "lol",  // LOLCODE
        "lookml",  // LookML
        "lpr",  // Pascal
        "ls",  // LiveScript, LoomScript
        "lsl",  // LSL
        "lslp",  // LSL
        "lsp",  // Common Lisp, NewLisp
        "ltx",  // TeX
        "lua",  // Lua
        "luau",  // Luau
        "lvclass",  // LabVIEW
        "lvlib",  // LabVIEW
        "lvproj",  // LabVIEW
        "ly",  // LilyPond
        "m",  // Limbo, M, MATLAB, ...
        "m2",  // Macaulay2
        "m3",  // Modula-3
        "m4",  // M4, M4Sugar
        "ma",  // Mathematica
        "mak",  // Makefile
        "make",  // Makefile
        "makefile",  // Makefile
        "mako",  // Mako
        "man",  // Roff, Roff Manpage
        "mao",  // Mako
        "markdown",  // Markdown
        "marko",  // Marko
        "mask",  // Mask, Unity3D Asset
        "mat",  // Unity3D Asset
        "mata",  // Stata
        "matah",  // Stata
        "mathematica",  // Mathematica
        "matlab",  // MATLAB
        "mawk",  // Awk
        "maxhelp",  // Max
        "maxpat",  // Max
        "maxproj",  // Max
        "mbox",  // E-mail
        "mbt",  // MoonBit
        "mc",  // M4, Monkey C, Win32 Message File
        "mcfunction",  // mcfunction
        "mcmeta",  // JSON
        "mcr",  // MAXScript
        "md",  // GCC Machine Description, Markdown
        "md2",  // Checksums
        "md4",  // Checksums
        "md5",  // Checksums
        "mdoc",  // Roff, Roff Manpage
        "mdown",  // Markdown
        "mdpolicy",  // XML
        "mdwn",  // Markdown
        "mdx",  // MDX
        "me",  // Roff
        "mediawiki",  // Wikitext
        "mermaid",  // Mermaid
        "meta",  // Unity3D Asset
        "metal",  // Metal
        "mg",  // Modula-3
        "minid",  // MiniD
        "mint",  // Mint
        "mir",  // YAML
        "mirah",  // Mirah
        "mjml",  // XML
        "mjs",  // JavaScript
        "mk",  // Makefile
        "mkd",  // Markdown
        "mkdn",  // Markdown
        "mkdown",  // Markdown
        "mkfile",  // Makefile
        "mkii",  // TeX
        "mkiv",  // TeX
        "mkvi",  // TeX
        "ml",  // OCaml, Standard ML
        "ml4",  // OCaml
        "mli",  // OCaml
        "mligo",  // CameLIGO
        "mlir",  // MLIR
        "mll",  // OCaml
        "mly",  // OCaml
        "mm",  // Objective-C++, XML
        "mmd",  // Mermaid
        "mmk",  // Module Management System
        "mms",  // Module Management System
        "mo",  // Modelica, Motoko
        "mod",  // AMPL, Linux Kernel Module, Modula-2, ...
        "mojo",  // Mojo, XML
        "monkey",  // Monkey
        "monkey2",  // Monkey
        "moo",  // Mercury, Moocode
        "moon",  // MoonScript
        "move",  // Move
        "mpl",  // JetBrains MPS
        "mps",  // JetBrains MPS
        "mq4",  // MQL4
        "mq5",  // MQL5
        "mqh",  // MQL4, MQL5
        "mrc",  // mIRC Script
        "ms",  // MAXScript, Roff, Unix Assembly
        "msd",  // JetBrains MPS
        "msg",  // omnetpp-msg
        "mspec",  // Ruby
        "mss",  // CartoCSS
        "mt",  // Mathematica
        "mtl",  // Wavefront Material
        "mtml",  // MTML
        "mts",  // TypeScript
        "mu",  // mupad
        "mud",  // ZIL
        "muf",  // MUF
        "mumps",  // M
        "muse",  // Muse
        "mustache",  // Mustache
        "mxml",  // XML
        "mxt",  // Max
        "mysql",  // SQL
        "myt",  // Myghty
        "n",  // Nemerle, Roff
        "nanorc",  // nanorc
        "nas",  // Assembly, Nasal
        "nasl",  // NASL
        "nasm",  // Assembly
        "natvis",  // XML
        "nawk",  // Awk
        "nb",  // Mathematica, Text
        "nbp",  // Mathematica
        "nc",  // nesC
        "ncl",  // Gerber Image, NCL, Text, ...
        "ndproj",  // XML
        "ne",  // Nearley
        "nearley",  // Nearley
        "ned",  // omnetpp-ned
        "neon",  // NEON
        "nf",  // Nextflow
        "nginx",  // Nginx
        "nginxconf",  // Nginx
        "ni",  // Inform 7
        "nim",  // Nim
        "nim.cfg",  // Nim
        "nimble",  // Nim
        "nimrod",  // Nim
        "nims",  // Nim
        "ninja",  // Ninja
        "nit",  // Nit
        "nix",  // Nix
        "njk",  // Nunjucks
        "njs",  // JavaScript
        "nl",  // NL, NewLisp
        "nlogo",  // NetLogo
        "no",  // Text
        "nomad",  // HCL
        "nproj",  // XML
        "nqp",  // Raku
        "nr",  // Noir, Roff
        "nse",  // Lua
        "nsh",  // NSIS
        "nsi",  // NSIS
        "nss",  // NWScript
        "nu",  // Nu, Nushell
        "numpy",  // NumPy
        "numpyw",  // NumPy
        "numsc",  // NumPy
        "nuspec",  // XML
        "nut",  // Squirrel
        "ny",  // Common Lisp
        "ob2",  // Oberon
        "obj",  // Wavefront Object
        "objdump",  // ObjDump
        "odd",  // XML
        "odin",  // Object Data Instance Notation, Odin
        "ol",  // Jolie
        "omgrofl",  // Omgrofl
        "ooc",  // ooc
        "opa",  // Opa
        "opal",  // Opal
        "opencl",  // OpenCL
        "orc",  // Csound
        "org",  // Org
        "os",  // 1C Enterprise
        "osm",  // XML
        "owl",  // Web Ontology Language
        "ox",  // Ox
        "oxh",  // Ox
        "oxo",  // Ox
        "oxygene",  // Oxygene
        "oz",  // Oz
        "p",  // Gnuplot, OpenEdge ABL
        "p4",  // P4
        "p6",  // Raku
        "p6l",  // Raku
        "p6m",  // Raku
        "p8",  // Lua
        "pac",  // JavaScript
        "pact",  // Pact
        "pan",  // Pan
        "parrot",  // Parrot
        "pas",  // Pascal
        "pascal",  // Pascal
        "pasm",  // Parrot Assembly
        "pat",  // Max
        "patch",  // Diff
        "pb",  // PureBasic
        "pbi",  // PureBasic
        "pbt",  // PowerBuilder, Protocol Buffer Text Format
        "pbtxt",  // Protocol Buffer Text Format
        "pck",  // PLSQL
        "pcss",  // PostCSS
        "pd",  // Pure Data
        "pd_lua",  // Lua
        "pddl",  // PDDL
        "pde",  // Processing
        "peggy",  // PEG.js
        "pegjs",  // PEG.js
        "pep",  // Pep8
        "per",  // Genero per
        "perl",  // Perl
        "pfa",  // PostScript
        "pgsql",  // PLpgSQL
        "ph",  // Perl
        "php",  // Hack, PHP
        "php3",  // PHP
        "php4",  // PHP
        "php5",  // PHP
        "phps",  // PHP
        "phpt",  // PHP
        "phtml",  // HTML+PHP
        "pic",  // Pic
        "pig",  // PigLatin
        "pike",  // Pike
        "pir",  // Parrot Internal Representation
        "pkb",  // PLSQL
        "pkgproj",  // XML
        "pkl",  // Pickle, Pkl
        "pks",  // PLSQL
        "pl",  // Perl, Prolog, Raku
        "pl6",  // Raku
        "plantuml",  // PlantUML
        "plb",  // PLSQL
        "plist",  // OpenStep Property List, XML Property List
        "plot",  // Gnuplot
        "pls",  // PLSQL
        "plsql",  // PLSQL
        "plt",  // Gnuplot, Prolog
        "pluginspec",  // Ruby, XML
        "plx",  // Perl
        "pm",  // Perl, Raku, X PixMap
        "pm6",  // Raku
        "pml",  // Promela
        "pmod",  // Pike
        "po",  // Gettext Catalog
        "pod",  // Pod, Pod 6
        "pod6",  // Pod 6
        "podsl",  // Common Lisp
        "podspec",  // Ruby
        "pogo",  // PogoScript
        "polar",  // Polar
        "pony",  // Pony
        "por",  // Portugol
        "postcss",  // PostCSS
        "pot",  // Gettext Catalog
        "pov",  // POV-Ray SDL
        "pp",  // Pascal, Puppet
        "pprx",  // REXX
        "praat",  // Praat
        "prawn",  // Ruby
        "prc",  // PLSQL, SQL
        "prefab",  // Unity3D Asset
        "prefs",  // INI
        "prg",  // xBase
        "pri",  // QMake
        "prisma",  // Prisma
        "pro",  // IDL, INI, Proguard, ...
        "proj",  // XML
        "prolog",  // Prolog
        "properties",  // INI, Java Properties
        "props",  // XML
        "proto",  // Protocol Buffer
        "prw",  // xBase
        "ps",  // PostScript
        "ps1",  // PowerShell
        "ps1xml",  // XML
        "psc",  // Papyrus
        "psc1",  // XML
        "psd1",  // PowerShell
        "psgi",  // Perl
        "psm1",  // PowerShell
        "pt",  // XML
        "pub",  // Public Key
        "pug",  // Pug
        "puml",  // PlantUML
        "purs",  // PureScript
        "pwn",  // Pawn
        "pxd",  // Cython
        "pxi",  // Cython
        "py",  // Python
        "py3",  // Python
        "pyde",  // Python
        "pyi",  // Python
        "pyp",  // Python
        "pyt",  // Python
        "pytb",  // Python traceback
        "pyw",  // Python
        "pyx",  // Cython
        "q",  // HiveQL, q
        "qasm",  // OpenQASM
        "qbs",  // QML
        "qhelp",  // XML
        "ql",  // CodeQL
        "qll",  // CodeQL
        "qmd",  // RMarkdown
        "qml",  // QML
        "qs",  // Q#, Qt Script
        "r",  // R, Rebol, Rez
        "r2",  // Rebol
        "r3",  // Rebol
        "rabl",  // Ruby
        "rake",  // Ruby
        "raku",  // Raku
        "rakumod",  // Raku
        "raml",  // RAML
        "raw",  // Raw token data
        "razor",  // HTML+Razor
        "rb",  // Ruby
        "rbbas",  // REALbasic
        "rbfrm",  // REALbasic
        "rbi",  // Ruby
        "rbmnu",  // REALbasic
        "rbres",  // REALbasic
        "rbs",  // RBS
        "rbtbar",  // REALbasic
        "rbuild",  // Ruby
        "rbuistate",  // REALbasic
        "rbw",  // Ruby
        "rbx",  // Ruby
        "rbxs",  // Lua
        "rchit",  // GLSL
        "rd",  // R
        "rdf",  // XML
        "rdoc",  // RDoc
        "re",  // C++, Reason
        "reb",  // Rebol
        "rebol",  // Rebol
        "red",  // Red
        "reds",  // Red
        "reek",  // YAML
        "reg",  // Windows Registry Entries
        "regex",  // Regular Expression
        "regexp",  // Regular Expression
        "rego",  // Open Policy Agent
        "rei",  // Reason
        "religo",  // ReasonLIGO
        "res",  // ReScript, XML
        "resource",  // RobotFramework
        "rest",  // reStructuredText
        "rest.txt",  // reStructuredText
        "resx",  // XML
        "rex",  // REXX
        "rexx",  // REXX
        "rg",  // Rouge
        "rhtml",  // HTML+ERB
        "ring",  // Ring
        "riot",  // Riot
        "rkt",  // Racket
        "rktd",  // Racket
        "rktl",  // Racket
        "rl",  // Ragel
        "rmd",  // RMarkdown
        "rmiss",  // GLSL
        "rnh",  // RUNOFF
        "rno",  // RUNOFF, Roff
        "rnw",  // Sweave
        "robot",  // RobotFramework
        "roc",  // Roc
        "rockspec",  // Lua
        "roff",  // Roff
        "ron",  // RON
        "ronn",  // Markdown
        "rpgle",  // RPGLE
        "rpy",  // Python, Ren'Py
        "rq",  // SPARQL
        "rs",  // RenderScript, Rust, XML
        "rs.in",  // Rust
        "rsc",  // Rascal, RouterOS Script
        "rsh",  // RenderScript
        "rss",  // XML
        "rst",  // reStructuredText
        "rst.txt",  // reStructuredText
        "rsx",  // R
        "rtf",  // Rich Text Format
        "ru",  // Ruby
        "ruby",  // Ruby
        "rviz",  // YAML
        "s",  // Motorola 68K Assembly, Unix Assembly
        "sage",  // Sage
        "sagews",  // Sage
        "sarif",  // JSON
        "sas",  // SAS
        "sass",  // Sass
        "sats",  // ATS
        "sbt",  // Scala
        "sc",  // Scala, SuperCollider
        "scad",  // OpenSCAD
        "scala",  // Scala
        "scaml",  // Scaml
        "scd",  // Markdown, SuperCollider
        "sce",  // Scilab
        "scenic",  // Scenic
        "sch",  // Eagle, KiCad Schematic, Scheme, ...
        "sci",  // Scilab
        "scm",  // Scheme
        "sco",  // Csound Score
        "scpt",  // AppleScript
        "scrbl",  // Racket
        "scss",  // SCSS
        "scxml",  // XML
        "sdc",  // Tcl
        "sed",  // sed
        "self",  // Self
        "service",  // desktop
        "sexp",  // Common Lisp
        "sfd",  // Spline Font Database
        "sfproj",  // XML
        "sfv",  // Simple File Verification
        "sh",  // Shell
        "sh-session",  // ShellSession
        "sh.in",  // Shell
        "sha1",  // Checksums
        "sha2",  // Checksums
        "sha224",  // Checksums
        "sha256",  // Checksums
        "sha256sum",  // Checksums
        "sha3",  // Checksums
        "sha384",  // Checksums
        "sha512",  // Checksums
        "shader",  // GLSL, ShaderLab
        "shen",  // Shen
        "shproj",  // XML
        "sieve",  // Sieve
        "sig",  // Standard ML
        "sj",  // Objective-J
        "sjs",  // JavaScript
        "sl",  // Slash
        "sld",  // Scheme
        "slim",  // Slim
        "slint",  // Slint
        "sln",  // Microsoft Visual Studio Solution
        "sls",  // SaltStack, Scheme
        "sma",  // Pawn
        "smali",  // Smali
        "smithy",  // Smithy
        "smk",  // Snakemake
        "sml",  // Standard ML
        "smt",  // SMT
        "smt2",  // SMT
        "snakefile",  // Snakemake
        "snap",  // Jest Snapshot
        "snip",  // Vim Snippet
        "snippet",  // Vim Snippet
        "snippets",  // Vim Snippet
        "sol",  // Gerber Image, Solidity
        "soy",  // Closure Templates
        "sp",  // SourcePawn
        "sparql",  // SPARQL
        "spc",  // PLSQL
        "spec",  // Python, RPM Spec, Ruby
        "spin",  // Propeller Spin
        "sps",  // Scheme
        "sqf",  // SQF
        "sql",  // PLSQL, PLpgSQL, SQL, ...
        "sqlrpgle",  // RPGLE
        "sra",  // PowerBuilder
        "srdf",  // XML
        "srt",  // SRecode Template, SubRip Text
        "sru",  // PowerBuilder
        "srw",  // PowerBuilder
        "ss",  // Scheme
        "ssjs",  // JavaScript
        "sss",  // SugarSS
        "st",  // Smalltalk, StringTemplate
        "stTheme",  // XML Property List
        "stan",  // Stan
        "star",  // STAR, Starlark
        "sthlp",  // Stata
        "stl",  // STL
        "ston",  // STON
        "story",  // Gherkin
        "storyboard",  // XML
        "sty",  // TeX
        "styl",  // Stylus
        "sublime-build",  // JSON with Comments
        "sublime-color-scheme",  // JSON with Comments
        "sublime-commands",  // JSON with Comments
        "sublime-completions",  // JSON with Comments
        "sublime-keymap",  // JSON with Comments
        "sublime-macro",  // JSON with Comments
        "sublime-menu",  // JSON with Comments
        "sublime-mousemap",  // JSON with Comments
        "sublime-project",  // JSON with Comments
        "sublime-settings",  // JSON with Comments
        "sublime-snippet",  // XML
        "sublime-syntax",  // YAML
        "sublime-theme",  // JSON with Comments
        "sublime-workspace",  // JSON with Comments
        "sublime_metrics",  // JSON with Comments
        "sublime_session",  // JSON with Comments
        "sv",  // SystemVerilog
        "svelte",  // Svelte
        "svg",  // SVG
        "svh",  // SystemVerilog
        "sw",  // Sway, XML
        "swift",  // Swift
        "syntax",  // YAML
        "t",  // Perl, Raku, Terra, ...
        "tab",  // SQL
        "tac",  // Python
        "tact",  // JSON, Tact
        "tag",  // Java Server Pages
        "talon",  // Talon
        "targets",  // XML
        "tcc",  // C++
        "tcl",  // Tcl
        "tcl.in",  // Tcl
        "tcsh",  // Tcsh
        "te",  // SELinux Policy
        "tea",  // Tea
        "templ",  // templ
        "tesc",  // GLSL
        "tese",  // GLSL
        "tex",  // TeX
        "texi",  // Texinfo
        "texinfo",  // Texinfo
        "textile",  // Textile
        "textproto",  // Protocol Buffer Text Format
        "tf",  // HCL
        "tfstate",  // JSON
        "tfstate.backup",  // JSON
        "tftpl",  // Terraform Template
        "tfvars",  // HCL
        "thor",  // Ruby
        "thrift",  // Thrift
        "thy",  // Isabelle
        "tl",  // Type Language
        "tla",  // TLA
        "tlv",  // TL-Verilog
        "tm",  // Tcl
        "tmCommand",  // XML Property List
        "tmLanguage",  // XML Property List
        "tmPreferences",  // XML Property List
        "tmSnippet",  // XML Property List
        "tmTheme",  // XML Property List
        "tmac",  // Roff
        "tml",  // XML
        "tmux",  // Shell
        "toc",  // TeX, World of Warcraft Addon Data
        "toit",  // Toit
        "toml",  // TOML
        "tool",  // Shell
        "topojson",  // JSON
        "tpb",  // PLSQL
        "tpl",  // Smarty
        "tpp",  // C++
        "tps",  // PLSQL
        "tres",  // Godot Resource
        "trg",  // PLSQL
        "trigger",  // Apex, Shell
        "ts",  // TypeScript, XML
        "tscn",  // Godot Resource
        "tsp",  // TSPLIB data, TypeSpec
        "tst",  // GAP, Scilab
        "tsv",  // TSV
        "tsx",  // TSX, XML
        "ttl",  // Turtle
        "tu",  // Turing
        "twig",  // Twig
        "txi",  // Texinfo
        "txl",  // TXL
        "txt",  // Adblock Filter List, Text, Vim Help File
        "txx",  // C++
        "typ",  // Typst, XML
        "uc",  // UnrealScript
        "udf",  // SQL
        "udo",  // Csound
        "ui",  // XML
        "unity",  // Unity3D Asset
        "uno",  // Uno
        "upc",  // Unified Parallel C
        "ur",  // UrWeb
        "urdf",  // XML
        "url",  // INI
        "urs",  // UrWeb
        "ux",  // XML
        "v",  // Coq, V, Verilog
        "vala",  // Vala
        "vapi",  // Vala
        "vark",  // Gosu
        "vb",  // Visual Basic .NET
        "vba",  // VBA, Vim Script
        "vbhtml",  // Visual Basic .NET
        "vbproj",  // XML
        "vbs",  // VBScript
        "vcf",  // TSV, vCard
        "vcl",  // VCL
        "vcxproj",  // XML
        "vdf",  // Valve Data Format
        "veo",  // Verilog
        "vert",  // GLSL
        "vh",  // SystemVerilog
        "vhd",  // VHDL
        "vhdl",  // VHDL
        "vhf",  // VHDL
        "vhi",  // VHDL
        "vho",  // VHDL
        "vhost",  // ApacheConf, Nginx
        "vhs",  // VHDL
        "vht",  // VHDL
        "vhw",  // VHDL
        "vim",  // Vim Script
        "vimrc",  // Vim Script
        "viw",  // SQL
        "vmb",  // Vim Script
        "volt",  // Volt
        "vrx",  // GLSL
        "vs",  // GLSL
        "vsh",  // GLSL
        "vshader",  // GLSL
        "vsixmanifest",  // XML
        "vssettings",  // XML
        "vstemplate",  // XML
        "vtl",  // Velocity Template Language
        "vtt",  // WebVTT
        "vue",  // Vue
        "vw",  // PLSQL
        "vxml",  // XML
        "vy",  // Vyper
        "w",  // CWeb, OpenEdge ABL
        "wast",  // WebAssembly
        "wat",  // WebAssembly
        "watchr",  // Ruby
        "wdl",  // WDL
        "webapp",  // JSON
        "webidl",  // WebIDL
        "webmanifest",  // JSON
        "weechatlog",  // IRC log
        "wgsl",  // WGSL
        "whiley",  // Whiley
        "wiki",  // Wikitext
        "wikitext",  // Wikitext
        "wisp",  // wisp
        "wit",  // WebAssembly Interface Type
        "wixproj",  // XML
        "wl",  // Mathematica
        "wlk",  // Wollok
        "wlt",  // Mathematica
        "wlua",  // Lua
        "workbook",  // Markdown
        "workflow",  // HCL, XML
        "wren",  // Wren
        "ws",  // Witcher Script
        "wsdl",  // XML
        "wsf",  // XML
        "wsgi",  // Python
        "wxi",  // XML
        "wxl",  // XML
        "wxs",  // XML
        "x",  // DirectX 3D File, Linker Script, Logos, ...
        "x10",  // X10
        "x3d",  // XML
        "x68",  // Motorola 68K Assembly
        "xacro",  // XML
        "xaml",  // XML
        "xbm",  // X BitMap
        "xc",  // XC
        "xdc",  // Tcl
        "xht",  // HTML
        "xhtml",  // HTML
        "xi",  // Logos
        "xib",  // XML
        "xlf",  // XML
        "xliff",  // XML
        "xm",  // Logos
        "xmi",  // XML
        "xml",  // XML
        "xml.dist",  // XML
        "xmp",  // XML
        "xojo_code",  // Xojo
        "xojo_menu",  // Xojo
        "xojo_report",  // Xojo
        "xojo_script",  // Xojo
        "xojo_toolbar",  // Xojo
        "xojo_window",  // Xojo
        "xpl",  // XProc
        "xpm",  // X PixMap
        "xproc",  // XProc
        "xproj",  // XML
        "xpy",  // Python
        "xq",  // XQuery
        "xql",  // XQuery
        "xqm",  // XQuery
        "xquery",  // XQuery
        "xqy",  // XQuery
        "xrl",  // Erlang
        "xs",  // XS
        "xsd",  // XML
        "xsh",  // Xonsh
        "xsjs",  // JavaScript
        "xsjslib",  // JavaScript
        "xsl",  // XSLT
        "xslt",  // XSLT
        "xsp-config",  // XPages
        "xsp.metadata",  // XPages
        "xspec",  // XML
        "xtend",  // Xtend
        "xul",  // XML
        "xzap",  // ZAP
        "y",  // Yacc
        "yacc",  // Yacc
        "yaml",  // MiniYAML, OASv2-yaml, OASv3-yaml, ...
        "yaml-tmlanguage",  // YAML
        "yaml.sed",  // YAML
        "yang",  // YANG
        "yap",  // Prolog
        "yar",  // YARA
        "yara",  // YARA
        "yasnippet",  // YASnippet
        "yml",  // MiniYAML, OASv2-yaml, OASv3-yaml, ...
        "yml.mysql",  // YAML
        "yrl",  // Erlang
        "yul",  // Yul
        "yy",  // JSON, Yacc
        "yyp",  // JSON
        "zap",  // ZAP
        "zcml",  // XML
        "zeek",  // Zeek
        "zep",  // Zephir
        "zig",  // Zig
        "zig.zon",  // Zig
        "zil",  // ZIL
        "zimpl",  // Zimpl
        "zmpl",  // Zimpl
        "zone",  // DNS Zone
        "zpl",  // Zimpl
        "zs",  // ZenScript
        "zsh",  // Shell
        "zsh-theme",  // Shell
    ]);

    set
});

/// Checks if a given path is a source code file based on its extension
pub fn is_source_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| SOURCE_EXTENSIONS.contains(ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get the total number of supported extensions
pub fn _supported_extension_count() -> usize {
    SOURCE_EXTENSIONS.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_source_detection() {
        let test_cases = vec![
            ("test.rs", true),
            ("test.py", true),
            ("test.js", true),
            ("test.xyz", false),
            ("test", false),
            ("test.txt", true),
            ("test.PY", true), // Should be case insensitive
        ];

        for (file, expected) in test_cases {
            let path = PathBuf::from(file);
            assert_eq!(is_source_file(&path), expected, "Failed for {}", file);
        }
    }
}
