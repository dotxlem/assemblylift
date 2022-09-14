require 'asml'
require 'base64'
require 'json'

def main(input)
    # TODO implement your function code here!
    Asml.success(input.to_s)
end

main(JSON.parse(Asml.get_function_input()))
