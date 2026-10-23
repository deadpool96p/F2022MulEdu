from __future__ import annotations
import collections.abc
import datetime
import typing
from warnings import deprecated # type: ignore

import jpype # type: ignore
import jpype.protocol # type: ignore

import ghidra.pty
import ghidra.pty.unix
import java.lang # type: ignore


class OpenBSDIoctls(java.lang.Enum[OpenBSDIoctls], ghidra.pty.unix.PosixC.Ioctls):

    class_: typing.ClassVar[java.lang.Class]
    INSTANCE: typing.Final[OpenBSDIoctls]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpenBSDIoctls:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpenBSDIoctls]:
        ...


class OpenBSDPtyFactory(java.lang.Enum[OpenBSDPtyFactory], ghidra.pty.PtyFactory):

    class_: typing.ClassVar[java.lang.Class]
    INSTANCE: typing.Final[OpenBSDPtyFactory]

    @staticmethod
    def valueOf(name: typing.Union[java.lang.String, str]) -> OpenBSDPtyFactory:
        ...

    @staticmethod
    def values() -> jpype.JArray[OpenBSDPtyFactory]:
        ...


class OpenBSDPtySessionLeader(ghidra.pty.unix.UnixPtySessionLeader):

    class_: typing.ClassVar[java.lang.Class]

    def __init__(self) -> None:
        ...

    @staticmethod
    def main(args: jpype.JArray[java.lang.String]) -> None:
        ...



__all__ = ["OpenBSDIoctls", "OpenBSDPtyFactory", "OpenBSDPtySessionLeader"]
