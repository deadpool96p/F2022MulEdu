from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import ghidra.pcode.emu.jit.decode
import ghidra.pcode.exec_
import ghidra.program.model.lang
import java.lang # type: ignore
import java.math # type: ignore


class MaskedBytes(java.lang.Object):
    """
    A masked array of bytes.
    
    
    This consists of a pair of byte arrays. One that represents the values, if known, and another
    that indicates which bits of the values array are actually known.
    """

    class_: typing.ClassVar[java.lang.Class]

    def and_(self, that: MaskedBytes) -> MaskedBytes:
        """
        Perform a bitwise and
        
        
        Note that known 0s in either operand cause known 0s in the result, even if the same bit of
        the other operand was unknown.
        
        :param MaskedBytes that: the other operand
        :return: the result
        :rtype: MaskedBytes
        """

    def bigMask(self, endian: ghidra.program.model.lang.Endian) -> java.math.BigInteger:
        """
        :return: the mask as an unsigned big integer
        :rtype: java.math.BigInteger
        
        
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        """

    def bigMaskSigned(self, endian: ghidra.program.model.lang.Endian) -> java.math.BigInteger:
        """
        :return: the mask as a signed big integer
        :rtype: java.math.BigInteger
        
        
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        """

    def bigValue(self, endian: ghidra.program.model.lang.Endian) -> java.math.BigInteger:
        """
        :return: the value as an unsigned big integer
        :rtype: java.math.BigInteger
        
        
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        """

    def bigValueSigned(self, endian: ghidra.program.model.lang.Endian) -> java.math.BigInteger:
        """
        :return: the value as a signed big integer
        :rtype: java.math.BigInteger
        
        
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        """

    def copyOfRange(self, start: typing.Union[jpype.JInt, int], stop: typing.Union[jpype.JInt, int]) -> MaskedBytes:
        """
        Copy a range of the masked bytes
        
        :param jpype.JInt or int start: the first byte to include
        :param jpype.JInt or int stop: the last byte, exclusive
        :return: the copy
        :rtype: MaskedBytes
        """

    def floatAbs(self, endian: ghidra.program.model.lang.Endian) -> MaskedBytes:
        """
        Zero out the first bit
        
        
        The first bit becomes a known 0 in the result, even if it was unknown.
        
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        :return: the result
        :rtype: MaskedBytes
        """

    def floatNeg(self, endian: ghidra.program.model.lang.Endian) -> MaskedBytes:
        """
        Invert the first bit, if it is known
        
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        :return: the result
        :rtype: MaskedBytes
        """

    def isMasked(self) -> bool:
        """
        :return: true if *any* portion, even a single bit, of the value is unknown
        :rtype: bool
        """

    def negate(self) -> MaskedBytes:
        """
        Invert the known bits
        
        :return: the new masked bytes
        :rtype: MaskedBytes
        """

    @staticmethod
    @typing.overload
    def of(val: jpype.JArray[jpype.JByte], msk: jpype.JArray[jpype.JByte]) -> MaskedBytes:
        """
        Construct from arrays
        
        
        The given arrays are copied, and the mask is applied so that all unknown bits have 0s in the
        values array.
        
        :param jpype.JArray[jpype.JByte] val: the value array
        :param jpype.JArray[jpype.JByte] msk: the mask array
        :return: the masked bytes
        :rtype: MaskedBytes
        """

    @staticmethod
    @typing.overload
    def of(val: java.math.BigInteger, msk: java.math.BigInteger, length: typing.Union[jpype.JInt, int], endian: ghidra.program.model.lang.Endian) -> MaskedBytes:
        """
        Construct from big integers
        
        :param java.math.BigInteger val: the value
        :param java.math.BigInteger msk: the bits of the value that are known
        :param jpype.JInt or int length: the size of the value in bytes
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        :return: the masked bytes
        :rtype: MaskedBytes
        """

    @staticmethod
    @typing.overload
    def of(val: typing.Union[jpype.JLong, int], msk: typing.Union[jpype.JLong, int], length: typing.Union[jpype.JInt, int], endian: ghidra.program.model.lang.Endian) -> MaskedBytes:
        """
        Construct from longs
        
        :param jpype.JLong or int val: the value
        :param jpype.JLong or int msk: the bits of the value that are known
        :param jpype.JInt or int length: the size of the value in bytes
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        :return: the masked bytes
        :rtype: MaskedBytes
        """

    @staticmethod
    def ofKnown(val: jpype.JArray[jpype.JByte]) -> MaskedBytes:
        """
        Construct from known bytes
        
        
        The resulting mask is all 1s.
        
        :param jpype.JArray[jpype.JByte] val: the known values
        :return: the masked bytes
        :rtype: MaskedBytes
        """

    @staticmethod
    def ofUnknown(length: typing.Union[jpype.JInt, int]) -> MaskedBytes:
        """
        Construct completely unknown
        
        
        The contents of both arrays (value and mask) are all 0s
        
        :param jpype.JInt or int length: the number of bytes
        :return: the masked bytes
        :rtype: MaskedBytes
        """

    def or_(self, that: MaskedBytes) -> MaskedBytes:
        """
        Perform a bitwise or
        
        
        Note that known 1s in either operand cause known ss in the result, even if the same bit of
        the other operand was unknown.
        
        :param MaskedBytes that: the other operand
        :return: the result
        :rtype: MaskedBytes
        """

    def sext(self, size: typing.Union[jpype.JInt, int], endian: ghidra.program.model.lang.Endian) -> MaskedBytes:
        """
        Perform sign extension
        
        
        The sign bit (known 0, known 1, or unknown) is extended to fill the requested size.
        
        :param jpype.JInt or int size: the size in bytes of the result
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        :return: the extended masked bytes
        :rtype: MaskedBytes
        """

    def xor(self, that: MaskedBytes) -> MaskedBytes:
        """
        Perform a bitwise xor
        
        :param MaskedBytes that: the other operand
        :return: the result
        :rtype: MaskedBytes
        """

    def zext(self, size: typing.Union[jpype.JInt, int], endian: ghidra.program.model.lang.Endian) -> MaskedBytes:
        """
        Perform zero/unsigned extension
        
        
        Known 0s fill to the requested size
        
        :param jpype.JInt or int size: the size in bytes of the result
        :param ghidra.program.model.lang.Endian endian: the endianness of the bytes
        :return: the extended masked bytes
        :rtype: MaskedBytes
        """

    @property
    def masked(self) -> jpype.JBoolean:
        ...


class FoldedState(ghidra.pcode.exec_.PcodeExecutorState[MaskedBytes]):
    """
    A p-code executor state on :obj:`MaskedBytes`.
    
    
    This is used as the state for interpretation when computing folded constants. Because the JIT
    only elides passage through registers and uniques, those are the only spaces we actually store
    here. Otherwise, this is roughly equivalent to :obj:`BytesPcodeExecutorState`, but we store both
    the values and masks of the :obj:`MaskedBytes` into the backing byte arrays.
    """

    @typing.type_check_only
    class FoldedSpace(java.lang.Object):
        ...
        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, language: ghidra.program.model.lang.Language) -> None:
        """
        Construct a constant-folding state for the given language
        
        :param ghidra.program.model.lang.Language language: the language
        """

    def fork(self) -> FoldedState:
        """
        Fork this state, without callbacks, because they're not supported here.
        
        :return: the copy
        :rtype: FoldedState
        """

    def intersectInPlace(self, that: FoldedState) -> bool:
        """
        Merge the given state into this one, modifying this state in place
        
        
        This is the merging operation used by :obj:`FoldRevalidator`. Where we have known values
        (1-bits in the mask) the other must also have known values, or else we erase our mask (and
        value) bits. If the other also has known values at the same offset, the values must match, or
        else we erase. These checks and erasures are applied with bit-level granularity. Any erasure
        results in a return of ``true`` (without short circuiting).
        
        :param FoldedState that: the other state
        :return: true if this state changed as a result of this intersection
        :rtype: bool
        """


class FoldedArithmetic(java.lang.Enum[FoldedArithmetic], ghidra.pcode.exec_.PcodeArithmetic[MaskedBytes]):
    """
    The arithmetic for computing folded constants
    
    
    For the most part, the result of any operation is only known if all inputs are known. In that
    case, the result is equivalent to applying :obj:`BytesPcodeArithmetic`. Otherwise, the result is
    completely unknown. Special treatment is given to bitwise boolean operators, extension operators,
    and shift operators. We also give some special treatment for operators that only modify the sign
    bit, e.g., :obj:`PcodeOp.FLOAT_ABS`.
    """

    class_: typing.ClassVar[java.lang.Class]
    BE: typing.Final[FoldedArithmetic]
    """
    The big-endian instance
    """

    LE: typing.Final[FoldedArithmetic]
    """
    The little-endian instance
    """


    @staticmethod
    def forEndian(endian: ghidra.program.model.lang.Endian) -> FoldedArithmetic:
        """
        :return: the instance for the given endian
        :rtype: FoldedArithmetic
        
        
        :param ghidra.program.model.lang.Endian endian: the endian
        """

    @staticmethod
    def forLanguage(language: ghidra.program.model.lang.Language) -> FoldedArithmetic:
        """
        :return: the instance for the given language
        :rtype: FoldedArithmetic
        
        
        :param ghidra.program.model.lang.Language language: the language
        """

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> FoldedArithmetic:
        ...

    @staticmethod
    def values() -> jpype.JArray[FoldedArithmetic]:
        ...


class FoldingExecutor(ghidra.pcode.exec_.PcodeExecutor[MaskedBytes]):
    """
    An executor that records and/or removes folded constants into a passage decoder.
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, passage: ghidra.pcode.emu.jit.decode.DecoderForOnePassage, state: FoldedState) -> None:
        ...

    def execute(self, program: ghidra.pcode.exec_.PcodeProgram) -> None:
        """
        Interpret the given program with the passage decoder's userop library
        
        :param ghidra.pcode.exec_.PcodeProgram program: the p-code to interpret
        """


class FoldRevalidator(java.lang.Object):
    """
    A mechanism for re-validating the folded constants discovered during decode
    
    
    Because alternative flows into a block can be discovered during decode, after that block's
    computations were already examined for folded constants, blocks must generally be re-examined
    with all discovered flows considered. We also have to consider that a block that becomes an entry
    point into the passage may have *no* incoming folded constants.
    
    
    The algorithm is basically a forward traversal of block flows until we reach convergence. We
    haven't performed full control-flow analysis at this point, so we just follow branches and merge
    as we go. If the target is also an entry point, we erase all the folded constants in the "merged"
    state. Otherwise, we merge with the state at the target and re-queue it for interpretation.
    Merging consists of taking only those constants common to both states, and where those constants
    have the same value. If merging produces the same state as was already there, that target is
    *not* re-queued. Convergence occurs when the queue is empty. LATER: Prove it?
    """

    @typing.type_check_only
    class RevalidateExecutor(FoldingExecutor):
        """
        The interpreter for re-validation
        """

        class_: typing.ClassVar[java.lang.Class]


    @typing.type_check_only
    class SubProgram(ghidra.pcode.exec_.PcodeProgram):
        """
        A portion of a p-code program
        
        
        Because we are "extracting" from a given start to the next branch, we generally don't need to
        specify the end, as the interpreter will naturally end at that branch.
        """

        class_: typing.ClassVar[java.lang.Class]


    class_: typing.ClassVar[java.lang.Class]

    def __init__(self, passage: ghidra.pcode.emu.jit.decode.DecoderForOnePassage) -> None:
        """
        Begin re-validating a decoded passage
        
        :param ghidra.pcode.emu.jit.decode.DecoderForOnePassage passage: the (mutable) decoded passage
        """

    def revalidate(self) -> None:
        """
        Re-validate folded constants
        
        
        I had worried this algorithm could get expensive with loops with a counter. I thought perhaps
        with bit-level tricks in INT_ADD/SUB, the varnode would get invalidated one bit per
        re-interpretation of the loop. However, INT_ADD would invalidate all higher order bits of an
        unknown bit all at once.
        """



__all__ = ["MaskedBytes", "FoldedState", "FoldedArithmetic", "FoldingExecutor", "FoldRevalidator"]
