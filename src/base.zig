const std = @import("std");
const testing = std.testing;
pub const Ident = @import("base/Ident.zig");
pub const Module = @import("base/Module.zig");
pub const Region = @import("base/Region.zig");
pub const Package = @import("base/Package.zig");
pub const TagName = @import("base/TagName.zig");
pub const FieldName = @import("base/FieldName.zig");
pub const ModuleEnv = @import("base/ModuleEnv.zig");
pub const TypeVarName = @import("base/TypeVarName.zig");
pub const StringLiteral = @import("base/StringLiteral.zig");

pub const Recursive = enum {
    NotRecursive,
    Recursive,
    TailRecursive,
};

pub const CalledVia = enum {};

/// Represents a value written as-is in a Roc source file.
pub const Literal = union(enum) {
    Int: Int,
    Float: Float,
    Bool: bool,
    Str: StringLiteral.Idx,
    /// A crash with a textual message describing why a crash occurred.
    Crash: StringLiteral.Idx,

    /// An integer number literal.
    pub const Int = union(enum) {
        I8: i8,
        U8: u8,
        I16: i16,
        U16: u16,
        I32: i32,
        U32: u32,
        I64: i64,
        U64: u64,
        I128: i128,
        U128: u128,
    };

    /// A fractional number literal.
    pub const Float = union(enum) {
        F32: f32,
        F64: f64,
        // We represent Dec as a large integer divided by 10^18, which is the maximum
        // number of decimal places that allow lossless conversion of U64 to Dec.
        Dec: u128,
    };

    /// An integer or fractional number literal.
    pub const Num = union(enum) {
        Int: Int,
        Float: Float,
    };
};

pub fn serializeLiteral(lit: Literal, strings: *StringLiteral.Store, writer: std.io.AnyWriter) !void {
    switch (lit) {
        .Bool => |b| {
            if (b) {
                try writer.writeAll("true");
            } else {
                try writer.writeAll("false");
            }
        },
        .Crash => |idx| {
            try writer.print("crash: \"{s}\"", .{strings.get(idx)});
        },
        .Str => |idx| {
            try writer.print("\"{s}\"", .{strings.get(idx)});
        },
        .Int => |i| {
            switch (i) {
                inline else => |num| {
                    const tag_name = @tagName(@as(std.meta.Tag(Literal.Int), i));
                    var tag_name_lc_buff: [4]u8 = undefined;
                    const tag_name_lc = std.ascii.lowerString(&tag_name_lc_buff, tag_name);
                    try writer.print("{d}{s}", .{ num, tag_name_lc });
                },
            }
        },
        .Float => |f| {
            switch (f) {
                .F32 => |num| try writer.print("{d:.0}f32", .{num}),
                .F64 => |num| try writer.print("{d:.0}f64", .{num}),
                .Dec => |_| try writer.print("todo", .{}),
            }
        },
    }
}

test "serialize" {
    var buff = std.ArrayList(u8).init(testing.allocator);
    defer buff.deinit();

    var str_interner = StringLiteral.Store.init(testing.allocator);
    defer str_interner.deinit();

    var str_lit = "testing".*;
    const str_lit_idx = str_interner.insert(&str_lit);

    var crash_lit = "crashed".*;
    const crash_lit_idx = str_interner.insert(&crash_lit);

    const cases = .{
        .{ .lit = Literal{ .Bool = true }, .expected = "true" },
        .{ .lit = Literal{ .Bool = false }, .expected = "false" },

        .{ .lit = Literal{ .Int = .{ .U8 = 3 } }, .expected = "3u8" },
        .{ .lit = Literal{ .Int = .{ .I8 = 3 } }, .expected = "3i8" },
        .{ .lit = Literal{ .Int = .{ .U16 = 3 } }, .expected = "3u16" },
        .{ .lit = Literal{ .Int = .{ .I16 = 3 } }, .expected = "3i16" },
        .{ .lit = Literal{ .Int = .{ .U32 = 3 } }, .expected = "3u32" },
        .{ .lit = Literal{ .Int = .{ .I32 = 3 } }, .expected = "3i32" },
        .{ .lit = Literal{ .Int = .{ .U64 = 3 } }, .expected = "3u64" },
        .{ .lit = Literal{ .Int = .{ .I64 = 3 } }, .expected = "3i64" },
        .{ .lit = Literal{ .Int = .{ .U128 = 3 } }, .expected = "3u128" },
        .{ .lit = Literal{ .Int = .{ .I128 = 3 } }, .expected = "3i128" },

        .{ .lit = Literal{ .Float = .{ .F32 = 3 } }, .expected = "3f32" },
        .{ .lit = Literal{ .Float = .{ .F64 = 3 } }, .expected = "3f64" },
        .{ .lit = Literal{ .Float = .{ .Dec = 3 } }, .expected = "todo" },

        .{ .lit = Literal{ .Str = str_lit_idx }, .expected = "\"testing\"" },
        .{ .lit = Literal{ .Crash = crash_lit_idx }, .expected = "crash: \"crashed\"" },
    };

    inline for (cases) |case| {
        try serializeLiteral(case.lit, &str_interner, buff.writer().any());
        try testing.expectEqualStrings(case.expected, buff.items);
        buff.clearRetainingCapacity();
    }
}
