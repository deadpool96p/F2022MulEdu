from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import java.lang # type: ignore


class CountsPcodeOps(java.lang.Object):
    """
    An interface for things that count instructions and p-code ops
    """

    class_: typing.ClassVar[java.lang.Class]

    def instructionCount(self) -> int:
        """
        :return: the number of instructions counted
        :rtype: int
        """

    def trailingOpCount(self) -> int:
        """
        :return: the number of trailing p-code ops counted
        :rtype: int
        
        
        
        This represents partial execution of an instruction. Though :meth:`instructionCount() <.instructionCount>`
        *does* count the partial instruction, this indicates how much of that instruction was
        counted. This should provide enough information to replay an execution to exactly the same
        p-code op step.
        """


class PcodeOpCounter(CountsPcodeOps):
    """
    The most basic implementation of :obj:`CountsPcodeOps`
    """

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @typing.overload
    def count(self, instructions: typing.Union[jpype.JInt, int], trailingOps: typing.Union[jpype.JInt, int]) -> None:
        """
        Count some number of additional instructions and ops
        
        
        If there are no new instructions, then the trailing ops are added to those already counted.
        If there is 1 or more new instructions, those are added to those already counted, but
        trailing ops is reset to only count those given.
        
        :param jpype.JInt or int instructions: the number of instructions
        :param jpype.JInt or int trailingOps: the number of trailing ops
        """

    @typing.overload
    def count(self, counter: CountsPcodeOps) -> None:
        """
        Count some number of additional instructions and ops
        
        
        This works as in :meth:`count(int, int) <.count>` but the counts are taken from another counter
        
        :param CountsPcodeOps counter: the other counter
        """

    def countOp(self, startsInstruction: typing.Union[jpype.JBoolean, bool]) -> None:
        """
        Count one additional op
        
        :param jpype.JBoolean or bool startsInstruction: true if this op is the beginning of a new instruction
        """



__all__ = ["CountsPcodeOps", "PcodeOpCounter"]
