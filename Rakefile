# frozen_string_literal: true

require "bundler/gem_tasks"

desc "Regenerate assets/express-grammar.json from the expressir grammar"
task :"expressir:grammar:dump" do
  require "expressir/express/grammar/parser"
  json = Expressir::Express::Grammar::Parser.cached_grammar_json
  path = File.expand_path("assets/express-grammar.json", __dir__)
  File.write(path, "#{json}\n")
  puts "wrote #{path}"
end
