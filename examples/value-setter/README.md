# Value Setter Module

A minimal Sovereign SDK module for setting and retrieving a single `u32` value.

This module is intentionally simple and serves two key purposes:

## 1. An Educational Tool

This crate is the official hands-on example for the [Sovereign SDK Book](https://docs.sovereign.xyz/). 
In the [Quickstart](https://docs.sovereign.xyz/3-quickstart.html) chapter, you will take this module in its initial, insecure state and add access control logic. It is the foundation for learning the core concepts of module development.

If you are following the book, you don't need to do anything else here—just follow the instructions in the guide!

## 2. A Module Template

This crate also serves as a clean, boilerplate-free starting point for building your own custom modules. It contains the minimal necessary structure, allowing you to focus on your application's logic without deleting lots of pre-existing code.

## What's Included? (The Initial State)

The module starts in a very basic state, containing only:

*   A minimal `ValueSetter<S: Spec>` struct with a single state variable: `value: StateValue<u32>`.
*   A single `CallMessage` variant: `SetValue(u32)`.
*   A basic `call` method that allows any user to set the `value`.