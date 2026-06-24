<?php

namespace App;

class Noise21
{
    private function compute(): int
    {
        return 21;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
