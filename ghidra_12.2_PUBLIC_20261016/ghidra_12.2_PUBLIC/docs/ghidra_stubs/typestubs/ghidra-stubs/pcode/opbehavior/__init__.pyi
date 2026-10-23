from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import java.lang # type: ignore
import java.math # type: ignore


class OpBehavior(java.lang.Object):
    """
    The implementation of a p-code opcode
    """

    class_: typing.ClassVar[java.lang.Class]
    COPY: typing.Final[OpBehaviorCopy]
    LOAD: typing.Final[SpecialOpBehavior]
    STORE: typing.Final[SpecialOpBehavior]
    BRANCH: typing.Final[SpecialOpBehavior]
    CBRANCH: typing.Final[SpecialOpBehavior]
    BRANCHIND: typing.Final[SpecialOpBehavior]
    CALL: typing.Final[SpecialOpBehavior]
    CALLIND: typing.Final[SpecialOpBehavior]
    CALLOTHER: typing.Final[SpecialOpBehavior]
    RETURN: typing.Final[SpecialOpBehavior]
    MULTIEQUAL: typing.Final[SpecialOpBehavior]
    INDIRECT: typing.Final[SpecialOpBehavior]
    PIECE: typing.Final[OpBehaviorPiece]
    SUBPIECE: typing.Final[OpBehaviorSubpiece]
    INT_EQUAL: typing.Final[OpBehaviorEqual]
    INT_NOTEQUAL: typing.Final[OpBehaviorNotEqual]
    INT_LESS: typing.Final[OpBehaviorIntLess]
    INT_LESSEQUAL: typing.Final[OpBehaviorIntLessEqual]
    INT_SLESS: typing.Final[OpBehaviorIntSless]
    INT_SLESSEQUAL: typing.Final[OpBehaviorIntSlessEqual]
    INT_ZEXT: typing.Final[OpBehaviorIntZext]
    INT_SEXT: typing.Final[OpBehaviorIntSext]
    INT_ADD: typing.Final[OpBehaviorIntAdd]
    INT_SUB: typing.Final[OpBehaviorIntSub]
    INT_CARRY: typing.Final[OpBehaviorIntCarry]
    INT_SCARRY: typing.Final[OpBehaviorIntScarry]
    INT_SBORROW: typing.Final[OpBehaviorIntSborrow]
    INT_2COMP: typing.Final[OpBehaviorInt2Comp]
    INT_NEGATE: typing.Final[OpBehaviorIntNegate]
    INT_XOR: typing.Final[OpBehaviorIntXor]
    INT_AND: typing.Final[OpBehaviorIntAnd]
    INT_OR: typing.Final[OpBehaviorIntOr]
    INT_LEFT: typing.Final[OpBehaviorIntLeft]
    INT_RIGHT: typing.Final[OpBehaviorIntRight]
    INT_SRIGHT: typing.Final[OpBehaviorIntSright]
    INT_MULT: typing.Final[OpBehaviorIntMult]
    INT_DIV: typing.Final[OpBehaviorIntDiv]
    INT_SDIV: typing.Final[OpBehaviorIntSdiv]
    INT_REM: typing.Final[OpBehaviorIntRem]
    INT_SREM: typing.Final[OpBehaviorIntSrem]
    BOOL_NEGATE: typing.Final[OpBehaviorBoolNegate]
    BOOL_XOR: typing.Final[OpBehaviorBoolXor]
    BOOL_AND: typing.Final[OpBehaviorBoolAnd]
    BOOL_OR: typing.Final[OpBehaviorBoolOr]
    CAST: typing.Final[SpecialOpBehavior]
    PTRADD: typing.Final[SpecialOpBehavior]
    PTRSUB: typing.Final[SpecialOpBehavior]
    FLOAT_EQUAL: typing.Final[OpBehaviorFloatEqual]
    FLOAT_NOTEQUAL: typing.Final[OpBehaviorFloatNotEqual]
    FLOAT_LESS: typing.Final[OpBehaviorFloatLess]
    FLOAT_LESSEQUAL: typing.Final[OpBehaviorFloatLessEqual]
    FLOAT_NAN: typing.Final[OpBehaviorFloatNan]
    FLOAT_ADD: typing.Final[OpBehaviorFloatAdd]
    FLOAT_DIV: typing.Final[OpBehaviorFloatDiv]
    FLOAT_MULT: typing.Final[OpBehaviorFloatMult]
    FLOAT_SUB: typing.Final[OpBehaviorFloatSub]
    FLOAT_NEG: typing.Final[OpBehaviorFloatNeg]
    FLOAT_ABS: typing.Final[OpBehaviorFloatAbs]
    FLOAT_SQRT: typing.Final[OpBehaviorFloatSqrt]
    FLOAT_INT2FLOAT: typing.Final[OpBehaviorFloatInt2Float]
    FLOAT_FLOAT2FLOAT: typing.Final[OpBehaviorFloatFloat2Float]
    FLOAT_TRUNC: typing.Final[OpBehaviorFloatTrunc]
    FLOAT_CEIL: typing.Final[OpBehaviorFloatCeil]
    FLOAT_FLOOR: typing.Final[OpBehaviorFloatFloor]
    FLOAT_ROUND: typing.Final[OpBehaviorFloatRound]
    LZCOUNT: typing.Final[OpBehaviorLzcount]
    POPCOUNT: typing.Final[OpBehaviorPopcount]
    SEGMENTOP: typing.Final[SpecialOpBehavior]
    CPOOLREF: typing.Final[SpecialOpBehavior]
    NEW: typing.Final[SpecialOpBehavior]
    INSERT: typing.Final[SpecialOpBehavior]
    ZPULL: typing.Final[SpecialOpBehavior]
    SPULL: typing.Final[SpecialOpBehavior]

    def opcode(self) -> int:
        """
        :return: the opcode implemented
        :rtype: int
        """


class OpBehaviorCopy(java.lang.Enum[OpBehaviorCopy], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_COPY: typing.Final[OpBehaviorCopy]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorCopy:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorCopy]:
        ...


class UnaryOpBehavior(OpBehavior):
    """
    The implementation of a unary (1 input arg, 1 output) p-code operator
    """

    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def evaluateUnary(self, sizeout: typing.Union[jpype.JInt, int], sizein: typing.Union[jpype.JInt, int], unsignedIn1: typing.Union[jpype.JLong, int]) -> int:
        """
        Evaluate the unary operation using long data
        
        
        NOTE: if the operation overflows bits may be set beyond the specified sizeout. Even though
        results should be treated as unsigned it may be returned as a signed long value. It is
        expected that the returned result always be properly truncated by the caller since the
        evaluation may not - this is done to conserve emulation cycles.
        
        :param jpype.JInt or int sizeout: intended output size (bytes)
        :param jpype.JInt or int sizein: in1 size (bytes)
        :param jpype.JLong or int unsignedIn1: unsigned input 1
        :return: operation result.
        :rtype: int
        
        .. seealso::
        
            | :obj:`Utils.longToBytes(long, int, boolean)`
        
            | :obj:`Utils.bytesToLong(byte[], int, boolean)`
        """

    @typing.overload
    def evaluateUnary(self, sizeout: typing.Union[jpype.JInt, int], sizein: typing.Union[jpype.JInt, int], unsignedIn1: java.math.BigInteger) -> java.math.BigInteger:
        """
        Evaluate the unary operation using BigInteger data
        
        
        NOTE: if the operation overflows bits may be set beyond the specified sizeout. Even though
        results should be treated as unsigned it may be returned as a signed value. It is expected
        that the returned result always be properly truncated by the caller since the evaluation may
        not - this is done to conserve emulation cycles.
        
        :param jpype.JInt or int sizeout: intended output size (bytes)
        :param jpype.JInt or int sizein: in1 size (bytes)
        :param java.math.BigInteger unsignedIn1: unsigned input 1
        :return: operation result.
        :rtype: java.math.BigInteger
        
        .. seealso::
        
            | :obj:`Utils.bigIntegerToBytes(BigInteger, int, boolean)`
        
            | :obj:`Utils.bytesToBigInteger(byte[], int, boolean, boolean)`
        """


class SpecialOpBehavior(java.lang.Enum[SpecialOpBehavior], OpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_LOAD: typing.Final[SpecialOpBehavior]
    OP_STORE: typing.Final[SpecialOpBehavior]
    OP_BRANCH: typing.Final[SpecialOpBehavior]
    OP_CBRANCH: typing.Final[SpecialOpBehavior]
    OP_BRANCHIND: typing.Final[SpecialOpBehavior]
    OP_CALL: typing.Final[SpecialOpBehavior]
    OP_CALLIND: typing.Final[SpecialOpBehavior]
    OP_CALLOTHER: typing.Final[SpecialOpBehavior]
    OP_RETURN: typing.Final[SpecialOpBehavior]
    OP_MULTIEQUAL: typing.Final[SpecialOpBehavior]
    OP_INDIRECT: typing.Final[SpecialOpBehavior]
    OP_CAST: typing.Final[SpecialOpBehavior]
    OP_PTRADD: typing.Final[SpecialOpBehavior]
    OP_PTRSUB: typing.Final[SpecialOpBehavior]
    OP_SEGMENTOP: typing.Final[SpecialOpBehavior]
    OP_CPOOLREF: typing.Final[SpecialOpBehavior]
    OP_NEW: typing.Final[SpecialOpBehavior]
    OP_INSERT: typing.Final[SpecialOpBehavior]
    OP_ZPULL: typing.Final[SpecialOpBehavior]
    OP_SPULL: typing.Final[SpecialOpBehavior]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> SpecialOpBehavior:
        ...

    @staticmethod
    def values() -> jpype.JArray[SpecialOpBehavior]:
        ...


class OpBehaviorPiece(java.lang.Enum[OpBehaviorPiece], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_PIECE: typing.Final[OpBehaviorPiece]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorPiece:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorPiece]:
        ...


class BinaryOpBehavior(OpBehavior):
    """
    The implementation of a binary (2 input args, 1 output) p-code operator
    """

    class_: typing.ClassVar[java.lang.Class]

    @typing.overload
    def evaluateBinary(self, sizeout: typing.Union[jpype.JInt, int], sizein: typing.Union[jpype.JInt, int], unsignedIn1: typing.Union[jpype.JLong, int], unsignedIn2: typing.Union[jpype.JLong, int]) -> int:
        """
        Evaluate the binary operation using long data
        
        
        NOTE: if the operation overflows bits may be set beyond the specified sizeout. Even though
        results should be treated as unsigned it may be returned as a signed long value. It is
        expected that the returned result always be properly truncated by the caller since the
        evaluation may not - this is done to conserve emulation cycles.
        
        :param jpype.JInt or int sizeout: intended output size (bytes)
        :param jpype.JInt or int sizein: in1 size (bytes)
        :param jpype.JLong or int unsignedIn1: unsigned input 1
        :param jpype.JLong or int unsignedIn2: unsigned input 2
        :return: operation result.
        :rtype: int
        
        .. seealso::
        
            | :obj:`Utils.longToBytes(long, int, boolean)`
        
            | :obj:`Utils.bytesToLong(byte[], int, boolean)`
        """

    @typing.overload
    def evaluateBinary(self, sizeout: typing.Union[jpype.JInt, int], sizein: typing.Union[jpype.JInt, int], unsignedIn1: java.math.BigInteger, unsignedIn2: java.math.BigInteger) -> java.math.BigInteger:
        """
        Evaluate the binary operation using BigInteger data
        
        
        NOTE: if the operation overflows bits may be set beyond the specified sizeout. Even though
        results should be treated as unsigned it may be returned as a signed value. It is expected
        that the returned result always be properly truncated by the caller since the evaluation may
        not - this is done to conserve emulation cycles.
        
        :param jpype.JInt or int sizeout: intended output size (bytes)
        :param jpype.JInt or int sizein: in1 size (bytes)
        :param java.math.BigInteger unsignedIn1: unsigned input 1
        :param java.math.BigInteger unsignedIn2: unsigned input 2
        :return: operation result.
        :rtype: java.math.BigInteger
        
        .. seealso::
        
            | :obj:`Utils.bigIntegerToBytes(BigInteger, int, boolean)`
        
            | :obj:`Utils.bytesToBigInteger(byte[], int, boolean, boolean)`
        """


class OpBehaviorSubpiece(java.lang.Enum[OpBehaviorSubpiece], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_SUBPIECE: typing.Final[OpBehaviorSubpiece]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorSubpiece:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorSubpiece]:
        ...


class OpBehaviorEqual(java.lang.Enum[OpBehaviorEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_EQUAL: typing.Final[OpBehaviorEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorEqual]:
        ...


class OpBehaviorNotEqual(java.lang.Enum[OpBehaviorNotEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_NOTEQUAL: typing.Final[OpBehaviorNotEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorNotEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorNotEqual]:
        ...


class OpBehaviorIntLess(java.lang.Enum[OpBehaviorIntLess], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_LESS: typing.Final[OpBehaviorIntLess]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntLess:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntLess]:
        ...


class OpBehaviorIntLessEqual(java.lang.Enum[OpBehaviorIntLessEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_LESSEQUAL: typing.Final[OpBehaviorIntLessEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntLessEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntLessEqual]:
        ...


class OpBehaviorIntSless(java.lang.Enum[OpBehaviorIntSless], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SLESS: typing.Final[OpBehaviorIntSless]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSless:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSless]:
        ...


class OpBehaviorIntSlessEqual(java.lang.Enum[OpBehaviorIntSlessEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SLESSEQUAL: typing.Final[OpBehaviorIntSlessEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSlessEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSlessEqual]:
        ...


class OpBehaviorIntZext(java.lang.Enum[OpBehaviorIntZext], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_ZEXT: typing.Final[OpBehaviorIntZext]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntZext:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntZext]:
        ...


class OpBehaviorIntSext(java.lang.Enum[OpBehaviorIntSext], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SEXT: typing.Final[OpBehaviorIntSext]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSext:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSext]:
        ...


class OpBehaviorIntAdd(java.lang.Enum[OpBehaviorIntAdd], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_ADD: typing.Final[OpBehaviorIntAdd]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntAdd:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntAdd]:
        ...


class OpBehaviorIntSub(java.lang.Enum[OpBehaviorIntSub], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SUB: typing.Final[OpBehaviorIntSub]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSub:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSub]:
        ...


class OpBehaviorIntCarry(java.lang.Enum[OpBehaviorIntCarry], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_CARRY: typing.Final[OpBehaviorIntCarry]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntCarry:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntCarry]:
        ...


class OpBehaviorIntScarry(java.lang.Enum[OpBehaviorIntScarry], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SCARRY: typing.Final[OpBehaviorIntScarry]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntScarry:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntScarry]:
        ...


class OpBehaviorIntSborrow(java.lang.Enum[OpBehaviorIntSborrow], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SBORROW: typing.Final[OpBehaviorIntSborrow]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSborrow:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSborrow]:
        ...


class OpBehaviorInt2Comp(java.lang.Enum[OpBehaviorInt2Comp], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_2COMP: typing.Final[OpBehaviorInt2Comp]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorInt2Comp:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorInt2Comp]:
        ...


class OpBehaviorIntNegate(java.lang.Enum[OpBehaviorIntNegate], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_NEGATE: typing.Final[OpBehaviorIntNegate]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntNegate:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntNegate]:
        ...


class OpBehaviorIntXor(java.lang.Enum[OpBehaviorIntXor], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_XOR: typing.Final[OpBehaviorIntXor]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntXor:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntXor]:
        ...


class OpBehaviorIntAnd(java.lang.Enum[OpBehaviorIntAnd], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_AND: typing.Final[OpBehaviorIntAnd]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntAnd:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntAnd]:
        ...


class OpBehaviorIntOr(java.lang.Enum[OpBehaviorIntOr], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_OR: typing.Final[OpBehaviorIntOr]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntOr:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntOr]:
        ...


class OpBehaviorIntLeft(java.lang.Enum[OpBehaviorIntLeft], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_LEFT: typing.Final[OpBehaviorIntLeft]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntLeft:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntLeft]:
        ...


class OpBehaviorIntRight(java.lang.Enum[OpBehaviorIntRight], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_RIGHT: typing.Final[OpBehaviorIntRight]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntRight:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntRight]:
        ...


class OpBehaviorIntSright(java.lang.Enum[OpBehaviorIntSright], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SRIGHT: typing.Final[OpBehaviorIntSright]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSright:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSright]:
        ...


class OpBehaviorIntMult(java.lang.Enum[OpBehaviorIntMult], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_MULT: typing.Final[OpBehaviorIntMult]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntMult:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntMult]:
        ...


class OpBehaviorIntDiv(java.lang.Enum[OpBehaviorIntDiv], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_DEV: typing.Final[OpBehaviorIntDiv]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntDiv:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntDiv]:
        ...


class OpBehaviorIntSdiv(java.lang.Enum[OpBehaviorIntSdiv], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SDIV: typing.Final[OpBehaviorIntSdiv]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSdiv:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSdiv]:
        ...


class OpBehaviorIntRem(java.lang.Enum[OpBehaviorIntRem], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_REM: typing.Final[OpBehaviorIntRem]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntRem:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntRem]:
        ...


class OpBehaviorIntSrem(java.lang.Enum[OpBehaviorIntSrem], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_INT_SREM: typing.Final[OpBehaviorIntSrem]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorIntSrem:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorIntSrem]:
        ...


class OpBehaviorBoolNegate(java.lang.Enum[OpBehaviorBoolNegate], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_BOOL_NEGATE: typing.Final[OpBehaviorBoolNegate]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorBoolNegate:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorBoolNegate]:
        ...


class OpBehaviorBoolXor(java.lang.Enum[OpBehaviorBoolXor], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_BOOL_XOR: typing.Final[OpBehaviorBoolXor]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorBoolXor:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorBoolXor]:
        ...


class OpBehaviorBoolAnd(java.lang.Enum[OpBehaviorBoolAnd], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_BOOL_AND: typing.Final[OpBehaviorBoolAnd]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorBoolAnd:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorBoolAnd]:
        ...


class OpBehaviorBoolOr(java.lang.Enum[OpBehaviorBoolOr], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_BOOL_OR: typing.Final[OpBehaviorBoolOr]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorBoolOr:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorBoolOr]:
        ...


class OpBehaviorFloatEqual(java.lang.Enum[OpBehaviorFloatEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_EQUAL: typing.Final[OpBehaviorFloatEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatEqual]:
        ...


class OpBehaviorFloatNotEqual(java.lang.Enum[OpBehaviorFloatNotEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_NOTEQUAL: typing.Final[OpBehaviorFloatNotEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatNotEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatNotEqual]:
        ...


class OpBehaviorFloatLess(java.lang.Enum[OpBehaviorFloatLess], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_LESS: typing.Final[OpBehaviorFloatLess]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatLess:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatLess]:
        ...


class OpBehaviorFloatLessEqual(java.lang.Enum[OpBehaviorFloatLessEqual], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_LESSEQUAL: typing.Final[OpBehaviorFloatLessEqual]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatLessEqual:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatLessEqual]:
        ...


class OpBehaviorFloatNan(java.lang.Enum[OpBehaviorFloatNan], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_NAN: typing.Final[OpBehaviorFloatNan]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatNan:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatNan]:
        ...


class OpBehaviorFloatAdd(java.lang.Enum[OpBehaviorFloatAdd], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_ADD: typing.Final[OpBehaviorFloatAdd]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatAdd:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatAdd]:
        ...


class OpBehaviorFloatDiv(java.lang.Enum[OpBehaviorFloatDiv], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_DIV: typing.Final[OpBehaviorFloatDiv]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatDiv:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatDiv]:
        ...


class OpBehaviorFloatMult(java.lang.Enum[OpBehaviorFloatMult], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_MULT: typing.Final[OpBehaviorFloatMult]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatMult:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatMult]:
        ...


class OpBehaviorFloatSub(java.lang.Enum[OpBehaviorFloatSub], BinaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_SUB: typing.Final[OpBehaviorFloatSub]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatSub:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatSub]:
        ...


class OpBehaviorFloatNeg(java.lang.Enum[OpBehaviorFloatNeg], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_NEG: typing.Final[OpBehaviorFloatNeg]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatNeg:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatNeg]:
        ...


class OpBehaviorFloatAbs(java.lang.Enum[OpBehaviorFloatAbs], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_ABS: typing.Final[OpBehaviorFloatAbs]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatAbs:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatAbs]:
        ...


class OpBehaviorFloatSqrt(java.lang.Enum[OpBehaviorFloatSqrt], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_SQRT: typing.Final[OpBehaviorFloatSqrt]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatSqrt:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatSqrt]:
        ...


class OpBehaviorFloatInt2Float(java.lang.Enum[OpBehaviorFloatInt2Float], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_INT2FLOAT: typing.Final[OpBehaviorFloatInt2Float]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatInt2Float:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatInt2Float]:
        ...


class OpBehaviorFloatFloat2Float(java.lang.Enum[OpBehaviorFloatFloat2Float], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_FLOAT2FLOAT: typing.Final[OpBehaviorFloatFloat2Float]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatFloat2Float:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatFloat2Float]:
        ...


class OpBehaviorFloatTrunc(java.lang.Enum[OpBehaviorFloatTrunc], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_TRUNC: typing.Final[OpBehaviorFloatTrunc]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatTrunc:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatTrunc]:
        ...


class OpBehaviorFloatCeil(java.lang.Enum[OpBehaviorFloatCeil], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_CEIL: typing.Final[OpBehaviorFloatCeil]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatCeil:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatCeil]:
        ...


class OpBehaviorFloatFloor(java.lang.Enum[OpBehaviorFloatFloor], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_FLOOR: typing.Final[OpBehaviorFloatFloor]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatFloor:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatFloor]:
        ...


class OpBehaviorFloatRound(java.lang.Enum[OpBehaviorFloatRound], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_FLOAT_ROUND: typing.Final[OpBehaviorFloatRound]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorFloatRound:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorFloatRound]:
        ...


class OpBehaviorLzcount(java.lang.Enum[OpBehaviorLzcount], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_LZCOUNT: typing.Final[OpBehaviorLzcount]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorLzcount:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorLzcount]:
        ...


class OpBehaviorPopcount(java.lang.Enum[OpBehaviorPopcount], UnaryOpBehavior):

    class_: typing.ClassVar[java.lang.Class]
    OP_POPCOUNT: typing.Final[OpBehaviorPopcount]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpBehaviorPopcount:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpBehaviorPopcount]:
        ...


class OpBehaviorFactory(java.lang.Object):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @staticmethod
    def getOpBehavior(opcode: typing.Union[jpype.JInt, int]) -> OpBehavior:
        ...



__all__ = ["OpBehavior", "OpBehaviorCopy", "UnaryOpBehavior", "SpecialOpBehavior", "OpBehaviorPiece", "BinaryOpBehavior", "OpBehaviorSubpiece", "OpBehaviorEqual", "OpBehaviorNotEqual", "OpBehaviorIntLess", "OpBehaviorIntLessEqual", "OpBehaviorIntSless", "OpBehaviorIntSlessEqual", "OpBehaviorIntZext", "OpBehaviorIntSext", "OpBehaviorIntAdd", "OpBehaviorIntSub", "OpBehaviorIntCarry", "OpBehaviorIntScarry", "OpBehaviorIntSborrow", "OpBehaviorInt2Comp", "OpBehaviorIntNegate", "OpBehaviorIntXor", "OpBehaviorIntAnd", "OpBehaviorIntOr", "OpBehaviorIntLeft", "OpBehaviorIntRight", "OpBehaviorIntSright", "OpBehaviorIntMult", "OpBehaviorIntDiv", "OpBehaviorIntSdiv", "OpBehaviorIntRem", "OpBehaviorIntSrem", "OpBehaviorBoolNegate", "OpBehaviorBoolXor", "OpBehaviorBoolAnd", "OpBehaviorBoolOr", "OpBehaviorFloatEqual", "OpBehaviorFloatNotEqual", "OpBehaviorFloatLess", "OpBehaviorFloatLessEqual", "OpBehaviorFloatNan", "OpBehaviorFloatAdd", "OpBehaviorFloatDiv", "OpBehaviorFloatMult", "OpBehaviorFloatSub", "OpBehaviorFloatNeg", "OpBehaviorFloatAbs", "OpBehaviorFloatSqrt", "OpBehaviorFloatInt2Float", "OpBehaviorFloatFloat2Float", "OpBehaviorFloatTrunc", "OpBehaviorFloatCeil", "OpBehaviorFloatFloor", "OpBehaviorFloatRound", "OpBehaviorLzcount", "OpBehaviorPopcount", "OpBehaviorFactory"]
